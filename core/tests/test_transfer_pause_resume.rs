//! Integration tests for the pause/resume feature of the QUIC transfer module.
//!
//! Two transfer servers (A = sender, B = receiver) are spun up on random
//! loopback ports in one process. A sends a multi-MB file to B; B auto-accepts
//! into a temp dir. Mid-flight, one side pauses; we assert BOTH sides reach
//! `Paused` (not Error/Cancelled), the partial file + resume state survive,
//! and a subsequent `resume_transfer` drives the transfer to `AllDone` with the
//! received bytes equal to the source.

use std::io::Write as _;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anydrop::transfer::{
    start_server, Decision, Direction, ProgressUpdate, ServerHandle, TransferOffer, TransferStatus,
};

/// A few MB so the pause reliably lands while bytes are still flowing on
/// loopback. Deterministic, non-repeating-ish content so a byte compare is
/// meaningful.
const FILE_SIZE: usize = 16 * 1024 * 1024;

fn make_test_file(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    // Pseudo-random but reproducible bytes via a simple LCG.
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut buf = vec![0u8; 64 * 1024];
    let mut written = 0usize;
    while written < FILE_SIZE {
        for b in buf.iter_mut() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            *b = (state >> 33) as u8;
        }
        let n = (FILE_SIZE - written).min(buf.len());
        f.write_all(&buf[..n]).unwrap();
        written += n;
    }
    f.flush().unwrap();
    path
}

/// Collected progress updates, shared between the on_progress callback and the
/// test thread.
type Log = Arc<Mutex<Vec<ProgressUpdate>>>;

/// Spin up a server bound to a random loopback port. `save_root`, when set,
/// makes every offer auto-accepted into that directory.
fn spawn_server(save_root: Option<std::path::PathBuf>) -> (Arc<ServerHandle>, Log) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();

    // on_offer needs the handle (to call `respond`), but the handle doesn't
    // exist yet. Bridge via a slot filled in right after start_server returns.
    let handle_slot: Arc<Mutex<Option<Arc<ServerHandle>>>> = Arc::new(Mutex::new(None));

    let offer_slot = handle_slot.clone();
    let save_root_for_offer = save_root.clone();
    let on_offer = move |offer: TransferOffer| {
        if let Some(root) = save_root_for_offer.clone() {
            if let Some(h) = offer_slot.lock().unwrap().as_ref() {
                h.respond(offer.transfer_id, Decision::Accept { save_root: root });
            }
        }
    };

    let log_for_progress = log.clone();
    let on_progress = move |u: ProgressUpdate| {
        log_for_progress.lock().unwrap().push(u);
    };

    let handle = start_server(bind, "test".to_string(), on_offer, on_progress).unwrap();
    let handle = Arc::new(handle);
    *handle_slot.lock().unwrap() = Some(handle.clone());
    // Keep the slot alive for the whole process so on_offer keeps working.
    Box::leak(Box::new(handle_slot));
    (handle, log)
}

fn count_status(log: &Log, dir: Direction, status: TransferStatus) -> usize {
    log.lock()
        .unwrap()
        .iter()
        .filter(|u| u.direction == dir && u.status == status)
        .count()
}

fn has_status(log: &Log, dir: Direction, status: TransferStatus) -> bool {
    count_status(log, dir, status) > 0
}

/// Largest `total_done` reported in the given direction so far.
fn max_total_done(log: &Log, dir: Direction) -> u64 {
    log.lock()
        .unwrap()
        .iter()
        .filter(|u| u.direction == dir)
        .map(|u| u.total_done)
        .max()
        .unwrap_or(0)
}

fn wait_for<F: Fn() -> bool>(timeout: Duration, f: F) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if f() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Wait until at least `min` bytes have flowed (mid-flight) but the transfer
/// hasn't already finished. Returns true if a good pause window was hit.
fn wait_midflight(send_log: &Log, recv_log: &Log, min: u64, total: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        // If either side already completed, we missed the window.
        if has_status(send_log, Direction::Send, TransferStatus::AllDone)
            || has_status(recv_log, Direction::Recv, TransferStatus::AllDone)
        {
            return false;
        }
        let done = max_total_done(recv_log, Direction::Recv);
        if done >= min && done < total {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn sender_pause_midflight_then_resume_completes() {
    pause_resume_case(PauseSide::Sender);
}

#[test]
fn receiver_pause_midflight_then_resume_completes() {
    pause_resume_case(PauseSide::Receiver);
}

enum PauseSide {
    Sender,
    Receiver,
}

fn pause_resume_case(side: PauseSide) {
    // Temp dirs for source and destination.
    let src_dir = std::env::temp_dir().join(format!("anydrop_src_{}", rand_suffix()));
    let dst_dir = std::env::temp_dir().join(format!("anydrop_dst_{}", rand_suffix()));
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();

    let src_file = make_test_file(&src_dir, "payload.bin");
    let src_bytes = std::fs::read(&src_file).unwrap();
    let total = src_bytes.len() as u64;

    let (sender, send_log) = spawn_server(None);
    let (receiver, recv_log) = spawn_server(Some(dst_dir.clone()));

    let recv_addr = receiver.local_addr();

    let transfer_id = sender.send_paths(recv_addr, vec![src_file.clone()]);

    // Wait until ~25% has flowed so the pause lands well mid-flight.
    let mid = total / 4;
    assert!(
        wait_midflight(&send_log, &recv_log, mid, total),
        "transfer never reached a mid-flight window to pause at"
    );

    // Pause from the chosen side.
    let paused_ok = match side {
        PauseSide::Sender => sender.pause_transfer(transfer_id),
        PauseSide::Receiver => receiver.pause_transfer(transfer_id),
    };
    assert!(paused_ok, "pause_transfer returned false (no such transfer)");

    // BOTH sides must reach Paused (not Error/Cancelled).
    assert!(
        wait_for(Duration::from_secs(15), || has_status(
            &send_log,
            Direction::Send,
            TransferStatus::Paused
        )),
        "sender did not reach Paused"
    );
    assert!(
        wait_for(Duration::from_secs(15), || has_status(
            &recv_log,
            Direction::Recv,
            TransferStatus::Paused
        )),
        "receiver did not reach Paused"
    );

    // Neither side should have errored or cancelled.
    assert!(
        !has_status(&send_log, Direction::Send, TransferStatus::Error),
        "sender erroneously reported Error on pause"
    );
    assert!(
        !has_status(&recv_log, Direction::Recv, TransferStatus::Error),
        "receiver erroneously reported Error on pause"
    );
    assert!(
        !has_status(&send_log, Direction::Send, TransferStatus::Cancelled),
        "sender erroneously reported Cancelled on pause"
    );
    assert!(
        !has_status(&recv_log, Direction::Recv, TransferStatus::Cancelled),
        "receiver erroneously reported Cancelled on pause"
    );

    // The receiver's partial file must survive on disk with some bytes.
    let partial_path = dst_dir.join("payload.bin");
    assert!(
        partial_path.exists(),
        "receiver's partial file does not exist"
    );
    let partial_len = std::fs::metadata(&partial_path).unwrap().len();
    assert!(
        partial_len > 0 && partial_len < total,
        "partial file length {} is not mid-flight (total {})",
        partial_len,
        total
    );

    // Give the connections a moment to fully tear down before resuming.
    std::thread::sleep(Duration::from_millis(300));

    // Resume — re-fires the send with the same id; receiver resumes from
    // already-received offsets.
    assert!(
        sender.resume_transfer(transfer_id),
        "resume_transfer returned false — resume state was destroyed"
    );

    // Both sides should now complete.
    assert!(
        wait_for(Duration::from_secs(40), || has_status(
            &send_log,
            Direction::Send,
            TransferStatus::AllDone
        )),
        "sender did not reach AllDone after resume"
    );
    assert!(
        wait_for(Duration::from_secs(40), || has_status(
            &recv_log,
            Direction::Recv,
            TransferStatus::AllDone
        )),
        "receiver did not reach AllDone after resume"
    );

    // The received file must equal the source bytes exactly.
    assert!(
        wait_for(Duration::from_secs(10), || {
            std::fs::metadata(&partial_path)
                .map(|m| m.len() == total)
                .unwrap_or(false)
        }),
        "received file never reached the expected size"
    );
    let received = std::fs::read(&partial_path).unwrap();
    assert_eq!(
        received.len(),
        src_bytes.len(),
        "received file size mismatch"
    );
    assert!(received == src_bytes, "received bytes differ from source");

    sender.close();
    receiver.close();
    let _ = std::fs::remove_dir_all(&src_dir);
    let _ = std::fs::remove_dir_all(&dst_dir);
}

fn rand_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    // Mix in a thread-local-ish counter via address of a stack var.
    let x = 0u8;
    nanos ^ ((&x as *const u8 as u64).rotate_left(17))
}
