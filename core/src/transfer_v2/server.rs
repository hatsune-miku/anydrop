//! Receiver side of transfer_v2.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{error, info, warn};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

use super::protocol::{Hello, HelloAck, Item, Status, DATA_HEADER_LEN};
use super::walk::sanitize_rel;
use super::{
    Decision, Direction, OnOffer, OnProgress, PendingOffers, ProgressUpdate, TransferOffer,
    TransferStatus,
};

const PROGRESS_INTERVAL_MS: u128 = 200;
const PROGRESS_BYTES: u64 = 1 << 20; // 1 MiB

/// Main accept loop.
pub(crate) async fn run(
    endpoint: quinn::Endpoint,
    pending: Arc<PendingOffers>,
    on_offer: OnOffer,
    on_progress: OnProgress,
) {
    loop {
        let Some(incoming) = endpoint.accept().await else {
            info!("transfer_v2: endpoint closed");
            return;
        };
        let pending = pending.clone();
        let on_offer = on_offer.clone();
        let on_progress = on_progress.clone();
        tokio::spawn(async move {
            let conn = match incoming.await {
                Ok(c) => c,
                Err(e) => {
                    warn!("transfer_v2: handshake failed: {}", e);
                    return;
                }
            };
            if let Err(e) = handle_connection(conn, pending, on_offer, on_progress).await {
                warn!("transfer_v2: connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(
    conn: quinn::Connection,
    pending: Arc<PendingOffers>,
    on_offer: OnOffer,
    on_progress: OnProgress,
) -> Result<(), String> {
    let remote_addr = conn.remote_address();
    let (mut ctrl_send, mut ctrl_recv) = conn
        .accept_bi()
        .await
        .map_err(|e| format!("accept ctrl: {}", e))?;

    // Read Hello.
    let hello: Hello = read_msg(&mut ctrl_recv).await?;
    let total_size: u64 = hello.items.iter().filter(|i| !i.is_dir).map(|i| i.size).sum();
    let items = Arc::new(hello.items.clone());

    let offer = TransferOffer {
        transfer_id: hello.transfer_id,
        remote_addr,
        display_name: hello.display_name.clone(),
        items: hello.items.clone(),
        total_size,
    };

    // Register oneshot and notify host.
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

    // Wait for decision with a sane timeout.
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
            };
            write_msg(&mut ctrl_send, &ack).await?;
            let _ = ctrl_send.finish();
            on_progress(ProgressUpdate {
                transfer_id: hello.transfer_id,
                direction: Direction::Recv,
                remote_addr,
                display_name: hello.display_name,
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
        Decision::Accept { save_root } => {
            let ack = HelloAck {
                accepted: true,
                reject_reason: None,
            };
            write_msg(&mut ctrl_send, &ack).await?;

            // Create all directories upfront.
            create_dirs(&save_root, &items).await;

            // Receive files. Accept as many unidirectional streams as there
            // are file items.
            let file_count = items.iter().filter(|i| !i.is_dir).count();
            let total_done = Arc::new(AtomicU64::new(0));
            let mut tasks = Vec::with_capacity(file_count);

            for _ in 0..file_count {
                let uni = match conn.accept_uni().await {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("transfer_v2: accept_uni: {}", e);
                        break;
                    }
                };
                let items_c = items.clone();
                let total_done_c = total_done.clone();
                let on_progress_c = on_progress.clone();
                let save_root_c = save_root.clone();
                let display_name_c = hello.display_name.clone();
                let transfer_id = hello.transfer_id;
                tasks.push(tokio::spawn(async move {
                    if let Err(e) = receive_one_file(
                        uni,
                        transfer_id,
                        items_c,
                        save_root_c,
                        remote_addr,
                        display_name_c,
                        total_size,
                        total_done_c,
                        on_progress_c,
                    )
                    .await
                    {
                        warn!("transfer_v2: file recv error: {}", e);
                    }
                }));
            }

            for t in tasks {
                let _ = t.await;
            }

            // Final AllDone notice.
            let _ = write_msg(&mut ctrl_send, &Status::AllDone).await;
            let _ = ctrl_send.finish();

            on_progress(ProgressUpdate {
                transfer_id: hello.transfer_id,
                direction: Direction::Recv,
                remote_addr,
                display_name: hello.display_name,
                item_idx: 0,
                rel_path: String::new(),
                item_size: 0,
                bytes_done: total_size,
                total_size,
                total_done: total_done.load(Ordering::Relaxed),
                status: TransferStatus::AllDone,
                error: None,
            });
        }
    }
    Ok(())
}

async fn create_dirs(save_root: &Path, items: &[Item]) {
    for item in items {
        if !item.is_dir {
            continue;
        }
        if let Some(rel) = sanitize_rel(&item.rel_path) {
            let abs = save_root.join(rel);
            if let Err(e) = tokio::fs::create_dir_all(&abs).await {
                warn!("transfer_v2: mkdir {:?}: {}", abs, e);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn receive_one_file(
    mut uni: quinn::RecvStream,
    transfer_id: u64,
    items: Arc<Vec<Item>>,
    save_root: PathBuf,
    remote_addr: std::net::SocketAddr,
    display_name: String,
    total_size: u64,
    total_done: Arc<AtomicU64>,
    on_progress: OnProgress,
) -> Result<(), String> {
    // Read header.
    let mut hdr = [0u8; DATA_HEADER_LEN];
    uni.read_exact(&mut hdr)
        .await
        .map_err(|e| format!("read header: {}", e))?;
    let stream_transfer_id = u64::from_be_bytes(hdr[0..8].try_into().unwrap());
    let item_idx = u32::from_be_bytes(hdr[8..12].try_into().unwrap());
    if stream_transfer_id != transfer_id {
        return Err(format!(
            "stream transfer_id mismatch: {} != {}",
            stream_transfer_id, transfer_id
        ));
    }
    let item = items
        .get(item_idx as usize)
        .cloned()
        .ok_or_else(|| format!("item_idx {} out of range", item_idx))?;
    if item.is_dir {
        return Err("data stream targets a directory item".into());
    }
    let rel = sanitize_rel(&item.rel_path)
        .ok_or_else(|| format!("unsafe rel_path: {}", item.rel_path))?;
    let abs = save_root.join(&rel);
    if let Some(parent) = abs.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let mut file = tokio::fs::File::create(&abs)
        .await
        .map_err(|e| format!("create {:?}: {}", abs, e))?;

    let mut buf = vec![0u8; 64 * 1024];
    let mut written: u64 = 0;
    let mut last_emit = Instant::now();
    let mut last_bytes: u64 = 0;

    loop {
        let n = uni
            .read(&mut buf)
            .await
            .map_err(|e| format!("read: {}", e))?;
        let n = match n {
            Some(n) if n > 0 => n,
            _ => break,
        };
        file.write_all(&buf[..n])
            .await
            .map_err(|e| format!("write: {}", e))?;
        written += n as u64;
        let total_done_now = total_done.fetch_add(n as u64, Ordering::Relaxed) + n as u64;

        let now = Instant::now();
        if now.duration_since(last_emit).as_millis() >= PROGRESS_INTERVAL_MS
            || written - last_bytes >= PROGRESS_BYTES
        {
            last_emit = now;
            last_bytes = written;
            on_progress(ProgressUpdate {
                transfer_id,
                direction: Direction::Recv,
                remote_addr,
                display_name: display_name.clone(),
                item_idx,
                rel_path: item.rel_path.clone(),
                item_size: item.size,
                bytes_done: written,
                total_size,
                total_done: total_done_now,
                status: TransferStatus::InProgress,
                error: None,
            });
        }
    }
    file.flush().await.map_err(|e| format!("flush: {}", e))?;

    on_progress(ProgressUpdate {
        transfer_id,
        direction: Direction::Recv,
        remote_addr,
        display_name,
        item_idx,
        rel_path: item.rel_path.clone(),
        item_size: item.size,
        bytes_done: written,
        total_size,
        total_done: total_done.load(Ordering::Relaxed),
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
    serde_json::from_slice(&body).map_err(|e| {
        error!("transfer_v2: bad json: {}", e);
        format!("json: {}", e)
    })
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
