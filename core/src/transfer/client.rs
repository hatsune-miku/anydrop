//! Sender side of the QUIC transfer protocol.
//!
//! On transient network failures (connection dropped, peer briefly
//! unreachable), the outer loop reconnects with the same `transfer_id` and
//! lets the receiver tell us, via `HelloAck.resume_offsets`, exactly how many
//! bytes per item it already has. We then skip ahead in each source file and
//! stream the remainder.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{info, warn};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::sync::CancellationToken;

use super::cert::client_config;
use super::protocol::{Hello, HelloAck, Item, Status, DATA_HEADER_LEN};
use super::server::{read_msg, write_msg};
use super::walk::walk_paths;
use super::{Direction, OnProgress, ProgressUpdate, TransferStatus};

const PROGRESS_INTERVAL_MS: u128 = 200;
const PROGRESS_BYTES: u64 = 1 << 20;

const MAX_ATTEMPTS: u32 = 5;
const BACKOFF_INITIAL_MS: u64 = 1_000;
const BACKOFF_CAP_MS: u64 = 16_000;

pub(crate) async fn send_paths_impl(
    transfer_id: u64,
    target: SocketAddr,
    paths: Vec<PathBuf>,
    display_name: String,
    on_progress: OnProgress,
    cancel: CancellationToken,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("no paths".into());
    }
    let walked = walk_paths(&paths);
    if walked.is_empty() {
        return Err("nothing to send (paths do not exist?)".into());
    }
    let items: Vec<Item> = walked.iter().map(|(i, _)| i.clone()).collect();
    let src_paths: Vec<Option<PathBuf>> = walked.iter().map(|(_, p)| p.clone()).collect();
    let total_size: u64 = items.iter().filter(|i| !i.is_dir).map(|i| i.size).sum();

    // Per-item bytes successfully streamed; survives across retries.
    let bytes_sent: Arc<Vec<AtomicU64>> =
        Arc::new(items.iter().map(|_| AtomicU64::new(0)).collect());

    info!(
        "transfer: sending {} items ({} bytes) to {}",
        items.len(),
        total_size,
        target
    );

    // Seed the host UI with a transfer-summary label before any per-item
    // progress fires — otherwise the row gets named after the first file in
    // the list, which is wrong/confusing when the user picked a folder.
    on_progress(ProgressUpdate {
        transfer_id,
        direction: Direction::Send,
        remote_addr: target,
        display_name: display_name.clone(),
        item_idx: 0,
        rel_path: build_summary_label(&items),
        item_size: 0,
        bytes_done: 0,
        total_size,
        total_done: 0,
        status: TransferStatus::InProgress,
        error: None,
    });

    let mut attempt: u32 = 0;
    let mut last_error: Option<String>;
    loop {
        // Cancel check before each attempt. `cancel_transfer` (terminal) and
        // `pause_transfer` (resumable) both fire this token; we tell them
        // apart by checking whether send_args was retained — but here we
        // only know we should stop. The caller (mod.rs) emits the correct
        // terminal status (Cancelled vs Paused) based on its own bookkeeping.
        if cancel.is_cancelled() {
            return emit_cancelled(
                transfer_id,
                &target,
                &display_name,
                total_size,
                &bytes_sent,
                &on_progress,
            );
        }
        attempt += 1;
        match attempt_send(
            target,
            transfer_id,
            &items,
            &src_paths,
            bytes_sent.clone(),
            total_size,
            display_name.clone(),
            on_progress.clone(),
            cancel.clone(),
        )
        .await
        {
            Ok(AttemptOutcome::Done) => return Ok(()),
            Ok(AttemptOutcome::Cancelled) => {
                return emit_cancelled(
                    transfer_id,
                    &target,
                    &display_name,
                    total_size,
                    &bytes_sent,
                    &on_progress,
                );
            }
            Ok(AttemptOutcome::Rejected(reason)) => {
                on_progress(ProgressUpdate {
                    transfer_id,
                    direction: Direction::Send,
                    remote_addr: target,
                    display_name: display_name.clone(),
                    item_idx: 0,
                    rel_path: String::new(),
                    item_size: 0,
                    bytes_done: 0,
                    total_size,
                    total_done: 0,
                    status: TransferStatus::Rejected,
                    error: Some(reason),
                });
                return Ok(());
            }
            Ok(AttemptOutcome::Aborted(reason)) => {
                let total_done: u64 =
                    bytes_sent.iter().map(|c| c.load(Ordering::Relaxed)).sum();
                on_progress(ProgressUpdate {
                    transfer_id,
                    direction: Direction::Send,
                    remote_addr: target,
                    display_name: display_name.clone(),
                    item_idx: 0,
                    rel_path: String::new(),
                    item_size: 0,
                    bytes_done: total_done,
                    total_size,
                    total_done,
                    status: TransferStatus::Error,
                    error: Some(reason.clone()),
                });
                return Err(reason);
            }
            Err(e) => {
                warn!(
                    "transfer: attempt {}/{} failed: {}",
                    attempt, MAX_ATTEMPTS, e
                );
                // Echo the per-attempt error into the progress stream so the
                // host UI's log can show it — otherwise the only signal a
                // user sees is the final aggregate failure after all retries.
                let total_done: u64 =
                    bytes_sent.iter().map(|c| c.load(Ordering::Relaxed)).sum();
                on_progress(ProgressUpdate {
                    transfer_id,
                    direction: Direction::Send,
                    remote_addr: target,
                    display_name: display_name.clone(),
                    item_idx: 0,
                    rel_path: String::new(),
                    item_size: 0,
                    bytes_done: total_done,
                    total_size,
                    total_done,
                    status: TransferStatus::InProgress,
                    error: Some(format!(
                        "attempt {}/{} failed: {}",
                        attempt, MAX_ATTEMPTS, e
                    )),
                });
                last_error = Some(e);
                if attempt >= MAX_ATTEMPTS {
                    break;
                }
                let backoff_ms = (BACKOFF_INITIAL_MS << (attempt - 1)).min(BACKOFF_CAP_MS);
                // Cancel during backoff exits without retrying.
                tokio::select! {
                    _ = cancel.cancelled() => {
                        return emit_cancelled(
                            transfer_id,
                            &target,
                            &display_name,
                            total_size,
                            &bytes_sent,
                            &on_progress,
                        );
                    }
                    _ = tokio::time::sleep(Duration::from_millis(backoff_ms)) => {}
                }
            }
        }
    }

    let err = last_error.unwrap_or_else(|| "send failed".into());
    on_progress(ProgressUpdate {
        transfer_id,
        direction: Direction::Send,
        remote_addr: target,
        display_name,
        item_idx: 0,
        rel_path: String::new(),
        item_size: 0,
        bytes_done: 0,
        total_size,
        total_done: bytes_sent.iter().map(|c| c.load(Ordering::Relaxed)).sum(),
        status: TransferStatus::Error,
        error: Some(err.clone()),
    });
    Err(err)
}

enum AttemptOutcome {
    Done,
    Rejected(String),
    /// Server received our streams but reported a permanent failure (e.g. it
    /// can't write a file because the name is illegal on its filesystem). No
    /// retry — the same error would happen again.
    Aborted(String),
    /// User cancelled via the host's `cancel_transfer` API mid-attempt. We
    /// stop everything and let the outer loop surface `Cancelled`.
    Cancelled,
}

/// Emit a TransferStatus::Cancelled progress update from the send-side and
/// return Ok(()). The bytes_sent snapshot becomes total_done so the UI can
/// show "stopped at X / Y bytes".
fn emit_cancelled(
    transfer_id: u64,
    target: &SocketAddr,
    display_name: &str,
    total_size: u64,
    bytes_sent: &[AtomicU64],
    on_progress: &OnProgress,
) -> Result<(), String> {
    let total_done: u64 = bytes_sent.iter().map(|c| c.load(Ordering::Relaxed)).sum();
    on_progress(ProgressUpdate {
        transfer_id,
        direction: Direction::Send,
        remote_addr: *target,
        display_name: display_name.to_string(),
        item_idx: 0,
        rel_path: String::new(),
        item_size: 0,
        bytes_done: total_done,
        total_size,
        total_done,
        status: TransferStatus::Cancelled,
        error: None,
    });
    Ok(())
}

/// Human-friendly label for the whole transfer, used by the host UI as the
/// row title. Avoids forward slashes so the host's `file_name()` basename
/// helper doesn't strip parts off.
fn build_summary_label(items: &[Item]) -> String {
    let files: Vec<&Item> = items.iter().filter(|i| !i.is_dir).collect();
    let dirs: Vec<&Item> = items.iter().filter(|i| i.is_dir).collect();

    // Pure single-file send.
    if files.len() == 1 && dirs.is_empty() {
        return last_segment(&files[0].rel_path);
    }
    // Folder(s) — use the shallowest directory's basename as the root name.
    if let Some(root) = dirs
        .iter()
        .min_by_key(|d| d.rel_path.trim_end_matches('/').matches('/').count())
    {
        let name = last_segment(root.rel_path.trim_end_matches('/'));
        return format!("{} ({} 个文件)", name, files.len());
    }
    // Multiple loose files.
    if files.is_empty() {
        return format!("传输 #{}", items.len());
    }
    let first = last_segment(&files[0].rel_path);
    format!("{} 等 {} 个文件", first, files.len())
}

fn last_segment(s: &str) -> String {
    s.rsplit('/').next().unwrap_or(s).to_string()
}

#[allow(clippy::too_many_arguments)]
async fn attempt_send(
    target: SocketAddr,
    transfer_id: u64,
    items: &[Item],
    src_paths: &[Option<PathBuf>],
    bytes_sent: Arc<Vec<AtomicU64>>,
    total_size: u64,
    display_name: String,
    on_progress: OnProgress,
    cancel: CancellationToken,
) -> Result<AttemptOutcome, String> {
    let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let mut endpoint =
        quinn::Endpoint::client(bind_addr).map_err(|e| format!("client endpoint: {}", e))?;
    endpoint.set_default_client_config(client_config()?);

    let conn = endpoint
        .connect(target, "anydrop")
        .map_err(|e| format!("connect: {}", e))?
        .await
        .map_err(|e| format!("handshake: {}", e))?;

    let (mut ctrl_send, mut ctrl_recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open ctrl: {}", e))?;
    let hello = Hello {
        transfer_id,
        display_name: display_name.clone(),
        items: items.to_vec(),
    };
    write_msg(&mut ctrl_send, &hello).await?;

    let ack: HelloAck = read_msg(&mut ctrl_recv).await?;
    if !ack.accepted {
        return Ok(AttemptOutcome::Rejected(
            ack.reject_reason.unwrap_or_else(|| "rejected".into()),
        ));
    }
    // Server is the source of truth: bump our counters to whatever it reports.
    for (idx, offset) in ack.resume_offsets {
        if let Some(counter) = bytes_sent.get(idx as usize) {
            let mut current = counter.load(Ordering::Relaxed);
            while offset > current {
                match counter.compare_exchange(
                    current,
                    offset,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => current = actual,
                }
            }
        }
    }

    for (idx, (item, src)) in items.iter().zip(src_paths.iter()).enumerate() {
        if item.is_dir {
            continue;
        }
        let Some(src) = src else {
            continue;
        };
        let already_sent = bytes_sent[idx].load(Ordering::Relaxed);
        if already_sent >= item.size {
            continue; // already fully transferred in a previous attempt
        }

        let mut uni = conn
            .open_uni()
            .await
            .map_err(|e| format!("open uni: {}", e))?;

        // 20-byte header.
        let mut hdr = [0u8; DATA_HEADER_LEN];
        hdr[0..8].copy_from_slice(&transfer_id.to_be_bytes());
        hdr[8..12].copy_from_slice(&(idx as u32).to_be_bytes());
        hdr[12..20].copy_from_slice(&already_sent.to_be_bytes());
        uni.write_all(&hdr)
            .await
            .map_err(|e| format!("write header: {}", e))?;

        let mut file = tokio::fs::File::open(src)
            .await
            .map_err(|e| format!("open {:?}: {}", src, e))?;
        if already_sent > 0 {
            file.seek(std::io::SeekFrom::Start(already_sent))
                .await
                .map_err(|e| format!("seek {:?}: {}", src, e))?;
        }

        let mut buf = vec![0u8; 64 * 1024];
        let mut bytes_this_stream: u64 = 0;
        let mut last_emit = Instant::now();
        let mut last_bytes: u64 = 0;

        loop {
            // Cancel-aware chunk loop. Check the token at the top of each
            // iteration; this gives ~64KB granularity for cancellation,
            // which at LAN speed is microseconds — plenty fast for "X" UX.
            if cancel.is_cancelled() {
                let _ = uni.finish();
                conn.close(0u32.into(), b"cancelled");
                let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.wait_idle()).await;
                return Ok(AttemptOutcome::Cancelled);
            }
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| format!("read src: {}", e))?;
            if n == 0 {
                break;
            }
            uni.write_all(&buf[..n])
                .await
                .map_err(|e| format!("send: {}", e))?;
            bytes_this_stream += n as u64;
            bytes_sent[idx].fetch_add(n as u64, Ordering::Relaxed);

            let total_done: u64 = bytes_sent.iter().map(|c| c.load(Ordering::Relaxed)).sum();

            let now = Instant::now();
            if now.duration_since(last_emit).as_millis() >= PROGRESS_INTERVAL_MS
                || bytes_this_stream - last_bytes >= PROGRESS_BYTES
            {
                last_emit = now;
                last_bytes = bytes_this_stream;
                on_progress(ProgressUpdate {
                    transfer_id,
                    direction: Direction::Send,
                    remote_addr: target,
                    display_name: display_name.clone(),
                    item_idx: idx as u32,
                    rel_path: item.rel_path.clone(),
                    item_size: item.size,
                    bytes_done: bytes_sent[idx].load(Ordering::Relaxed),
                    total_size,
                    total_done,
                    status: TransferStatus::InProgress,
                    error: None,
                });
            }
        }
        let _ = uni.finish();

        on_progress(ProgressUpdate {
            transfer_id,
            direction: Direction::Send,
            remote_addr: target,
            display_name: display_name.clone(),
            item_idx: idx as u32,
            rel_path: item.rel_path.clone(),
            item_size: item.size,
            bytes_done: bytes_sent[idx].load(Ordering::Relaxed),
            total_size,
            total_done: bytes_sent.iter().map(|c| c.load(Ordering::Relaxed)).sum(),
            status: TransferStatus::ItemDone,
            error: None,
        });
    }

    // Wait for the server's terminal Status message. Three relevant outcomes:
    //   * `AllDone` — every file landed cleanly, this attempt is a success
    //   * `Abort { reason }` — server hit a permanent error, surface it and
    //     skip the retry loop
    //   * read error / timeout — connection died mid-flight; bubble that up
    //     as an attempt-level error so the outer loop retries
    let final_status = tokio::time::timeout(
        Duration::from_secs(60),
        read_msg::<Status>(&mut ctrl_recv),
    )
    .await;
    let _ = ctrl_send.finish();

    match final_status {
        Ok(Ok(Status::AllDone)) => {
            on_progress(ProgressUpdate {
                transfer_id,
                direction: Direction::Send,
                remote_addr: target,
                display_name,
                item_idx: 0,
                rel_path: String::new(),
                item_size: 0,
                bytes_done: total_size,
                total_size,
                total_done: bytes_sent.iter().map(|c| c.load(Ordering::Relaxed)).sum(),
                status: TransferStatus::AllDone,
                error: None,
            });
            conn.close(0u32.into(), b"done");
            let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.wait_idle()).await;
            Ok(AttemptOutcome::Done)
        }
        Ok(Ok(Status::Abort { reason })) => {
            conn.close(0u32.into(), b"abort");
            let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.wait_idle()).await;
            Ok(AttemptOutcome::Aborted(reason))
        }
        Ok(Ok(Status::ItemDone { .. })) => {
            // Stray per-item ack at the end — shouldn't happen with our
            // protocol, but treat as a transient anomaly so the outer loop
            // retries rather than hanging.
            conn.close(0u32.into(), b"unexpected");
            Err("server sent ItemDone instead of terminal status".into())
        }
        Ok(Err(e)) => Err(format!("read final status: {}", e)),
        Err(_) => Err("timed out waiting for final status from server".into()),
    }
}
