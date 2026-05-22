//! Receiver side of the QUIC transfer protocol, with in-memory resume support.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{info, warn};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::protocol::{Hello, HelloAck, Item, Status, DATA_HEADER_LEN};
use super::walk::sanitize_rel;
use super::{
    Decision, Direction, OnOffer, OnProgress, PendingOffers, ProgressUpdate, TransferOffer,
    TransferStatus,
};

const PROGRESS_INTERVAL_MS: u128 = 200;
const PROGRESS_BYTES: u64 = 1 << 20; // 1 MiB
/// Inactivity window after which a stalled in-memory transfer is forgotten.
const ACTIVE_TRANSFER_TTL: Duration = Duration::from_secs(300);

/// State kept in memory for the lifetime of one in-flight transfer. Survives
/// the disconnection between Hello+ack and the data streams, so a sender that
/// reconnects with the same `transfer_id` can pick up where it left off.
pub(crate) struct ActiveTransfer {
    pub save_root: PathBuf,
    pub items: Arc<Vec<Item>>,
    pub item_bytes: Arc<Vec<AtomicU64>>,
    pub total_size: u64,
    pub last_activity: Mutex<Instant>,
}

pub(crate) type ActiveTransfersMap = Arc<Mutex<HashMap<u64, Arc<ActiveTransfer>>>>;

/// Type alias for the shared cancel-token registry that lives on
/// `ServerHandle`. Receiver-side transfers register their token here on
/// accept so the host's `cancel_transfer` API works symmetrically.
pub(crate) type CancelMap = Arc<Mutex<HashMap<u64, CancellationToken>>>;

/// Main accept loop.
pub(crate) async fn run(
    endpoint: quinn::Endpoint,
    pending: Arc<PendingOffers>,
    on_offer: OnOffer,
    on_progress: OnProgress,
    cancels: CancelMap,
) {
    let active: ActiveTransfersMap = Arc::new(Mutex::new(HashMap::new()));

    // Sweep stalled transfers so the map doesn't grow unbounded.
    {
        let active_for_sweep = active.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            tick.tick().await; // first tick fires immediately; skip it
            loop {
                tick.tick().await;
                let now = Instant::now();
                let mut map = active_for_sweep.lock().unwrap();
                map.retain(|id, t| {
                    let keep = t
                        .last_activity
                        .lock()
                        .map(|t| now.duration_since(*t) < ACTIVE_TRANSFER_TTL)
                        .unwrap_or(true);
                    if !keep {
                        info!("transfer: sweeping stale transfer_id={}", id);
                    }
                    keep
                });
            }
        });
    }

    loop {
        let Some(incoming) = endpoint.accept().await else {
            info!("transfer: endpoint closed");
            return;
        };
        let pending = pending.clone();
        let on_offer = on_offer.clone();
        let on_progress = on_progress.clone();
        let active = active.clone();
        let cancels = cancels.clone();
        tokio::spawn(async move {
            let conn = match incoming.await {
                Ok(c) => c,
                Err(e) => {
                    warn!("transfer: handshake failed: {}", e);
                    return;
                }
            };
            if let Err(e) =
                handle_connection(conn, pending, on_offer, on_progress, active, cancels).await
            {
                warn!("transfer: connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(
    conn: quinn::Connection,
    pending: Arc<PendingOffers>,
    on_offer: OnOffer,
    on_progress: OnProgress,
    active: ActiveTransfersMap,
    cancels: CancelMap,
) -> Result<(), String> {
    let remote_addr = conn.remote_address();
    let (mut ctrl_send, mut ctrl_recv) = conn
        .accept_bi()
        .await
        .map_err(|e| format!("accept ctrl: {}", e))?;

    let hello: Hello = read_msg(&mut ctrl_recv).await?;
    let transfer_id = hello.transfer_id;

    // Resume path: if we already have state for this transfer_id and the
    // items match, ACK immediately without re-prompting the user.
    let existing = active.lock().unwrap().get(&transfer_id).cloned();
    let entry = if let Some(prev) = existing.filter(|p| items_match(&p.items, &hello.items)) {
        info!(
            "transfer: resuming transfer_id={} from {}",
            transfer_id, remote_addr
        );
        *prev.last_activity.lock().unwrap() = Instant::now();
        let resume_offsets = prev
            .item_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let n = c.load(Ordering::Relaxed);
                if n > 0 {
                    Some((i as u32, n))
                } else {
                    None
                }
            })
            .collect();
        let ack = HelloAck {
            accepted: true,
            reject_reason: None,
            resume_offsets,
        };
        write_msg(&mut ctrl_send, &ack).await?;
        prev
    } else {
        let new_entry = match negotiate_new_offer(
            &hello,
            remote_addr,
            &pending,
            &on_offer,
            &on_progress,
            &mut ctrl_send,
        )
        .await?
        {
            Some(e) => e,
            None => {
                // User rejected. The HelloAck { accepted: false } is queued
                // in ctrl_send but not yet on the wire — wait for the client
                // to read it (= FIN on its half of the bi stream) before
                // closing, otherwise quinn's conn drop preempts the bytes
                // and the client sees "connection lost" → retry loop.
                let _ = ctrl_send.finish();
                await_client_finish_then_close(&conn, &mut ctrl_recv).await;
                return Ok(());
            }
        };
        create_dirs(&new_entry.save_root, &new_entry.items).await;
        active
            .lock()
            .unwrap()
            .insert(transfer_id, new_entry.clone());
        new_entry
    };

    // Register (or reuse) a cancel token for this transfer so the host's
    // `cancel_transfer` API can stop the receive tasks. We get-or-insert so
    // resume reuses the same token entry, while a fresh transfer creates one.
    let cancel_token = {
        let mut guard = cancels.lock().unwrap();
        guard
            .entry(transfer_id)
            .or_insert_with(CancellationToken::new)
            .clone()
    };

    // Receive data streams. Only items not already complete will arrive.
    let mut tasks = Vec::new();
    let file_count = entry.items.iter().filter(|i| !i.is_dir).count();
    let expected_remaining = entry
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| !it.is_dir)
        .filter(|(idx, it)| entry.item_bytes[*idx].load(Ordering::Relaxed) < it.size)
        .count();

    for _ in 0..expected_remaining {
        let uni = match conn.accept_uni().await {
            Ok(s) => s,
            Err(e) => {
                warn!("transfer: accept_uni: {}", e);
                emit_diagnostic(
                    &on_progress,
                    transfer_id,
                    remote_addr,
                    &hello.display_name,
                    &entry,
                    format!("accept_uni failed: {}", e),
                );
                break;
            }
        };
        let entry_c = entry.clone();
        let on_progress_c = on_progress.clone();
        let display_name_c = hello.display_name.clone();
        let cancel_c = cancel_token.clone();
        tasks.push(tokio::spawn(async move {
            receive_one_file(
                uni,
                transfer_id,
                entry_c,
                remote_addr,
                display_name_c,
                on_progress_c,
                cancel_c,
            )
            .await
        }));
    }

    // Collect per-task errors so we can both report them individually and
    // synthesize an overall abort reason.
    let mut task_errors: Vec<String> = Vec::new();
    for t in tasks {
        match t.await {
            Ok(Ok(())) => (),
            Ok(Err(e)) => {
                warn!("transfer: file recv error: {}", e);
                emit_diagnostic(
                    &on_progress,
                    transfer_id,
                    remote_addr,
                    &hello.display_name,
                    &entry,
                    format!("recv error: {}", e),
                );
                task_errors.push(e);
            }
            Err(join_err) => {
                warn!("transfer: recv task panicked: {}", join_err);
                emit_diagnostic(
                    &on_progress,
                    transfer_id,
                    remote_addr,
                    &hello.display_name,
                    &entry,
                    format!("recv task panic: {}", join_err),
                );
                task_errors.push(format!("task panic: {}", join_err));
            }
        }
    }

    *entry.last_activity.lock().unwrap() = Instant::now();
    let all_done = entry
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| !it.is_dir)
        .all(|(idx, it)| entry.item_bytes[idx].load(Ordering::Relaxed) >= it.size);

    // Errors trump completion: if any file failed (e.g. invalid filename on
    // NTFS, disk full, permission denied), we abort the whole transfer.
    // These failures are typically not transient — a retry would hit the
    // same wall — so we drop the in-memory state to prevent the client's
    // resume logic from looping forever on the same error.
    if !task_errors.is_empty() {
        let reason = if task_errors.len() == 1 {
            task_errors[0].clone()
        } else {
            format!(
                "{} (and {} more)",
                task_errors[0],
                task_errors.len() - 1
            )
        };
        warn!(
            "transfer: aborting transfer_id={} due to {} error(s); first: {}",
            transfer_id,
            task_errors.len(),
            task_errors[0]
        );
        active.lock().unwrap().remove(&transfer_id);
        cancels.lock().unwrap().remove(&transfer_id);
        let _ = write_msg(
            &mut ctrl_send,
            &Status::Abort {
                reason: reason.clone(),
            },
        )
        .await;
        let _ = ctrl_send.finish();
        let total_done: u64 = entry
            .item_bytes
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .sum();
        on_progress(ProgressUpdate {
            transfer_id,
            direction: Direction::Recv,
            remote_addr,
            display_name: hello.display_name,
            item_idx: 0,
            rel_path: String::new(),
            item_size: 0,
            bytes_done: total_done,
            total_size: entry.total_size,
            total_done,
            status: TransferStatus::Error,
            error: Some(reason),
        });
        await_client_finish_then_close(&conn, &mut ctrl_recv).await;
        return Ok(());
    }

    if all_done {
        active.lock().unwrap().remove(&transfer_id);
        cancels.lock().unwrap().remove(&transfer_id);
        let _ = write_msg(&mut ctrl_send, &Status::AllDone).await;
        let _ = ctrl_send.finish();
        let total_done = entry.total_size;
        on_progress(ProgressUpdate {
            transfer_id,
            direction: Direction::Recv,
            remote_addr,
            display_name: hello.display_name,
            item_idx: 0,
            rel_path: String::new(),
            item_size: 0,
            bytes_done: total_done,
            total_size: entry.total_size,
            total_done,
            status: TransferStatus::AllDone,
            error: None,
        });
        info!(
            "transfer: complete transfer_id={} ({} files, {} bytes)",
            transfer_id, file_count, total_done
        );
        await_client_finish_then_close(&conn, &mut ctrl_recv).await;
    } else {
        info!(
            "transfer: connection closed mid-flight for transfer_id={} — keeping state for resume",
            transfer_id
        );
        let _ = ctrl_send.finish();
        // Note: NOT closing here on purpose — we want the client to perceive
        // the abrupt connection drop as a transient failure and retry, which
        // the resume map will then service.
    }
    Ok(())
}

/// Wait for the client to acknowledge our final Status, *then* close.
///
/// `Connection::close()` is documented as immediate — pending sends fail
/// with `LocallyClosed` and the close frame races against our own queued
/// data. So we cannot close right after writing AllDone/Abort; we'd preempt
/// the very bytes the client is waiting on (the field symptom: client sees
/// "connection lost" instead of AllDone, even though the bulk file data on
/// the unidirectional streams arrived fine).
///
/// The client, in `attempt_send`, finishes its own `ctrl_send` half *after*
/// successfully reading our final Status. So if we read from `ctrl_recv`
/// and see EOF, we know the client has the bytes. Wait for that, then close.
async fn await_client_finish_then_close(
    conn: &quinn::Connection,
    ctrl_recv: &mut quinn::RecvStream,
) {
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        let mut buf = [0u8; 32];
        loop {
            match ctrl_recv.read(&mut buf).await {
                Ok(Some(_)) => continue, // unexpected stray bytes; drain & wait for FIN
                _ => break,              // EOF or error — either way we're done waiting
            }
        }
    })
    .await;
    conn.close(0u32.into(), b"done");
    let _ = tokio::time::timeout(Duration::from_secs(2), conn.closed()).await;
}

/// First-time negotiation: surface the offer, wait for user decision, ACK,
/// and (on accept) build a fresh `ActiveTransfer` entry.
async fn negotiate_new_offer(
    hello: &Hello,
    remote_addr: std::net::SocketAddr,
    pending: &Arc<PendingOffers>,
    on_offer: &OnOffer,
    on_progress: &OnProgress,
    ctrl_send: &mut quinn::SendStream,
) -> Result<Option<Arc<ActiveTransfer>>, String> {
    let total_size: u64 = hello.items.iter().filter(|i| !i.is_dir).map(|i| i.size).sum();
    let offer = TransferOffer {
        transfer_id: hello.transfer_id,
        remote_addr,
        display_name: hello.display_name.clone(),
        items: hello.items.clone(),
        total_size,
    };

    let (tx, rx) = oneshot::channel::<Decision>();
    pending.map.lock().unwrap().insert(hello.transfer_id, tx);
    on_offer(offer);
    on_progress(ProgressUpdate {
        transfer_id: hello.transfer_id,
        direction: Direction::Recv,
        remote_addr,
        display_name: hello.display_name.clone(),
        item_idx: 0,
        rel_path: String::new(),
        item_size: 0,
        bytes_done: 0,
        total_size,
        total_done: 0,
        status: TransferStatus::PendingDecision,
        error: None,
    });

    let decision = match tokio::time::timeout(Duration::from_secs(180), rx).await {
        Ok(Ok(d)) => d,
        _ => {
            pending.map.lock().unwrap().remove(&hello.transfer_id);
            Decision::Reject {
                reason: "no decision (timeout)".into(),
            }
        }
    };

    match decision {
        Decision::Reject { reason } => {
            let ack = HelloAck {
                accepted: false,
                reject_reason: Some(reason.clone()),
                resume_offsets: Vec::new(),
            };
            let _ = write_msg(ctrl_send, &ack).await;
            // NB: don't finish ctrl_send here; the caller (handle_connection)
            // does it inside await_client_finish_then_close so the HelloAck
            // actually makes it out before quinn tears the connection down.
            on_progress(ProgressUpdate {
                transfer_id: hello.transfer_id,
                direction: Direction::Recv,
                remote_addr,
                display_name: hello.display_name.clone(),
                item_idx: 0,
                rel_path: String::new(),
                item_size: 0,
                bytes_done: 0,
                total_size,
                total_done: 0,
                status: TransferStatus::Rejected,
                error: Some(reason),
            });
            Ok(None)
        }
        Decision::Accept { save_root } => {
            let ack = HelloAck {
                accepted: true,
                reject_reason: None,
                resume_offsets: Vec::new(),
            };
            write_msg(ctrl_send, &ack).await?;
            let items = Arc::new(hello.items.clone());
            let item_bytes: Vec<AtomicU64> =
                (0..items.len()).map(|_| AtomicU64::new(0)).collect();
            Ok(Some(Arc::new(ActiveTransfer {
                save_root,
                items,
                item_bytes: Arc::new(item_bytes),
                total_size,
                last_activity: Mutex::new(Instant::now()),
            })))
        }
    }
}

/// Push a diagnostic line through the progress channel.
///
/// Uses `TransferStatus::InProgress` with an `error` string — the host's log
/// surfaces any progress update that carries an error message, regardless of
/// status, so this lets server-side problems show up in the in-app log
/// without inventing a new event channel.
fn emit_diagnostic(
    on_progress: &OnProgress,
    transfer_id: u64,
    remote_addr: std::net::SocketAddr,
    display_name: &str,
    entry: &ActiveTransfer,
    msg: String,
) {
    let total_done: u64 = entry
        .item_bytes
        .iter()
        .map(|c| c.load(Ordering::Relaxed))
        .sum();
    on_progress(ProgressUpdate {
        transfer_id,
        direction: Direction::Recv,
        remote_addr,
        display_name: display_name.to_string(),
        item_idx: 0,
        rel_path: String::new(),
        item_size: 0,
        bytes_done: total_done,
        total_size: entry.total_size,
        total_done,
        status: TransferStatus::InProgress,
        error: Some(msg),
    });
}

/// Two transfers with the same id are "the same" iff their item lists match.
fn items_match(a: &[Item], b: &[Item]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| x.rel_path == y.rel_path && x.size == y.size && x.is_dir == y.is_dir)
}

async fn create_dirs(save_root: &Path, items: &[Item]) {
    for item in items {
        if !item.is_dir {
            continue;
        }
        if let Some(rel) = sanitize_rel(&item.rel_path) {
            let abs = save_root.join(rel);
            if let Err(e) = tokio::fs::create_dir_all(&abs).await {
                warn!("transfer: mkdir {:?}: {}", abs, e);
            }
        }
    }
}

async fn receive_one_file(
    mut uni: quinn::RecvStream,
    transfer_id: u64,
    entry: Arc<ActiveTransfer>,
    remote_addr: std::net::SocketAddr,
    display_name: String,
    on_progress: OnProgress,
    cancel: CancellationToken,
) -> Result<(), String> {
    // 20-byte header: transfer_id u64 BE + item_idx u32 BE + start_offset u64 BE.
    let mut hdr = [0u8; DATA_HEADER_LEN];
    uni.read_exact(&mut hdr)
        .await
        .map_err(|e| format!("read header: {}", e))?;
    let stream_transfer_id = u64::from_be_bytes(hdr[0..8].try_into().unwrap());
    let item_idx = u32::from_be_bytes(hdr[8..12].try_into().unwrap());
    let start_offset = u64::from_be_bytes(hdr[12..20].try_into().unwrap());
    if stream_transfer_id != transfer_id {
        return Err(format!(
            "stream transfer_id mismatch: {} != {}",
            stream_transfer_id, transfer_id
        ));
    }
    let item = entry
        .items
        .get(item_idx as usize)
        .cloned()
        .ok_or_else(|| format!("item_idx {} out of range", item_idx))?;
    if item.is_dir {
        return Err("data stream targets a directory item".into());
    }
    let rel = sanitize_rel(&item.rel_path)
        .ok_or_else(|| format!("unsafe rel_path: {}", item.rel_path))?;
    let abs = entry.save_root.join(&rel);
    if let Some(parent) = abs.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    info!(
        "transfer: recv start item_idx={} path={:?} offset={} size={}",
        item_idx, abs, start_offset, item.size
    );

    // Fresh start truncates; resume keeps existing bytes and seeks past them.
    let mut opts = OpenOptions::new();
    opts.write(true).create(true);
    if start_offset == 0 {
        opts.truncate(true);
    }
    let mut file = opts
        .open(&abs)
        .await
        .map_err(|e| format!("open {:?}: {}", abs, e))?;
    if start_offset > 0 {
        file.seek(std::io::SeekFrom::Start(start_offset))
            .await
            .map_err(|e| format!("seek {:?}: {}", abs, e))?;
    }

    // Counter is monotonic; only advance if the new value is higher.
    let counter = &entry.item_bytes[item_idx as usize];
    let mut current = counter.load(Ordering::Relaxed);
    while start_offset > current {
        match counter.compare_exchange(
            current,
            start_offset,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }

    let mut buf = vec![0u8; 64 * 1024];
    let mut bytes_this_stream: u64 = 0;
    let mut last_emit = Instant::now();
    let mut last_bytes: u64 = 0;

    loop {
        // Race the stream read against the cancellation token. If cancel
        // fires, return a sentinel error which handle_connection will fold
        // into task_errors → Abort path → Status::Abort to sender.
        let n = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return Err("cancelled by user".into());
            }
            res = uni.read(&mut buf) => res.map_err(|e| format!("read: {}", e))?,
        };
        let n = match n {
            Some(n) if n > 0 => n,
            _ => break,
        };
        file.write_all(&buf[..n])
            .await
            .map_err(|e| format!("write: {}", e))?;
        bytes_this_stream += n as u64;
        counter.fetch_add(n as u64, Ordering::Relaxed);
        let item_total = counter.load(Ordering::Relaxed);

        let total_done: u64 = entry
            .item_bytes
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .sum();

        let now = Instant::now();
        if now.duration_since(last_emit).as_millis() >= PROGRESS_INTERVAL_MS
            || bytes_this_stream - last_bytes >= PROGRESS_BYTES
        {
            last_emit = now;
            last_bytes = bytes_this_stream;
            *entry.last_activity.lock().unwrap() = now;
            on_progress(ProgressUpdate {
                transfer_id,
                direction: Direction::Recv,
                remote_addr,
                display_name: display_name.clone(),
                item_idx,
                rel_path: item.rel_path.clone(),
                item_size: item.size,
                bytes_done: item_total,
                total_size: entry.total_size,
                total_done,
                status: TransferStatus::InProgress,
                error: None,
            });
        }
    }
    file.flush().await.map_err(|e| format!("flush: {}", e))?;
    *entry.last_activity.lock().unwrap() = Instant::now();

    let item_total = counter.load(Ordering::Relaxed);
    let total_done: u64 = entry
        .item_bytes
        .iter()
        .map(|c| c.load(Ordering::Relaxed))
        .sum();
    info!(
        "transfer: recv done item_idx={} bytes={} (expected {})",
        item_idx, item_total, item.size
    );

    on_progress(ProgressUpdate {
        transfer_id,
        direction: Direction::Recv,
        remote_addr,
        display_name,
        item_idx,
        rel_path: item.rel_path.clone(),
        item_size: item.size,
        bytes_done: item_total,
        total_size: entry.total_size,
        total_done,
        status: TransferStatus::ItemDone,
        error: None,
    });
    Ok(())
}

/// Length-prefixed JSON read off a quinn RecvStream.
pub(crate) async fn read_msg<T: serde::de::DeserializeOwned>(
    recv: &mut quinn::RecvStream,
) -> Result<T, String> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| format!("read len: {}", e))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 8 * 1024 * 1024 {
        return Err(format!("message too large: {}", len));
    }
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body)
        .await
        .map_err(|e| format!("read body: {}", e))?;
    serde_json::from_slice(&body).map_err(|e| format!("json: {}", e))
}

/// Length-prefixed JSON write into a quinn SendStream.
pub(crate) async fn write_msg<T: serde::Serialize>(
    send: &mut quinn::SendStream,
    msg: &T,
) -> Result<(), String> {
    let body = serde_json::to_vec(msg).map_err(|e| format!("encode: {}", e))?;
    let len = (body.len() as u32).to_be_bytes();
    send.write_all(&len)
        .await
        .map_err(|e| format!("write len: {}", e))?;
    send.write_all(&body)
        .await
        .map_err(|e| format!("write body: {}", e))?;
    Ok(())
}
