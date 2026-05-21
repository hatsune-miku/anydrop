//! Sender side of transfer_v2.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::info;
use tokio::io::AsyncReadExt;

use super::cert::client_config;
use super::protocol::{Hello, HelloAck, DATA_HEADER_LEN};
use super::server::{read_msg, write_msg};
use super::walk::walk_paths;
use super::{Direction, OnProgress, ProgressUpdate, TransferStatus};

const PROGRESS_INTERVAL_MS: u128 = 200;
const PROGRESS_BYTES: u64 = 1 << 20;

pub(crate) async fn send_paths_impl(
    target: SocketAddr,
    paths: Vec<PathBuf>,
    display_name: String,
    on_progress: OnProgress,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("no paths".into());
    }
    let walked = walk_paths(&paths);
    if walked.is_empty() {
        return Err("nothing to send (paths do not exist?)".into());
    }
    let items: Vec<_> = walked.iter().map(|(i, _)| i.clone()).collect();
    let total_size: u64 = items.iter().filter(|i| !i.is_dir).map(|i| i.size).sum();
    let transfer_id: u64 = rand::random();

    info!(
        "transfer_v2: sending {} items ({} bytes) to {}",
        items.len(),
        total_size,
        target
    );

    // Build an ephemeral QUIC client endpoint.
    let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let mut endpoint =
        quinn::Endpoint::client(bind_addr).map_err(|e| format!("client endpoint: {}", e))?;
    endpoint.set_default_client_config(client_config()?);

    let conn = endpoint
        .connect(target, "anydrop")
        .map_err(|e| format!("connect: {}", e))?
        .await
        .map_err(|e| format!("handshake: {}", e))?;

    // Open the control bi stream.
    let (mut ctrl_send, mut ctrl_recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open ctrl: {}", e))?;
    let hello = Hello {
        transfer_id,
        display_name: display_name.clone(),
        items: items.clone(),
    };
    write_msg(&mut ctrl_send, &hello).await?;

    let ack: HelloAck = read_msg(&mut ctrl_recv).await?;
    if !ack.accepted {
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
            total_done: 0,
            status: TransferStatus::Rejected,
            error: ack.reject_reason,
        });
        let _ = ctrl_send.finish();
        return Ok(());
    }

    // Send each file as a unidirectional stream.
    let total_done = Arc::new(AtomicU64::new(0));
    for (idx, (item, src_path)) in walked.iter().enumerate() {
        if item.is_dir {
            continue;
        }
        let Some(src) = src_path else {
            continue;
        };
        let mut uni = conn
            .open_uni()
            .await
            .map_err(|e| format!("open uni: {}", e))?;

        // Header: transfer_id (u64 BE) + item_idx (u32 BE)
        let mut hdr = [0u8; DATA_HEADER_LEN];
        hdr[0..8].copy_from_slice(&transfer_id.to_be_bytes());
        hdr[8..12].copy_from_slice(&(idx as u32).to_be_bytes());
        uni.write_all(&hdr)
            .await
            .map_err(|e| format!("write header: {}", e))?;

        // Stream file bytes.
        let mut file = tokio::fs::File::open(src)
            .await
            .map_err(|e| format!("open {:?}: {}", src, e))?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut written: u64 = 0;
        let mut last_emit = Instant::now();
        let mut last_bytes: u64 = 0;

        loop {
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
                    direction: Direction::Send,
                    remote_addr: target,
                    display_name: display_name.clone(),
                    item_idx: idx as u32,
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
        // Signal end of stream to the receiver.
        let _ = uni.finish();

        on_progress(ProgressUpdate {
            transfer_id,
            direction: Direction::Send,
            remote_addr: target,
            display_name: display_name.clone(),
            item_idx: idx as u32,
            rel_path: item.rel_path.clone(),
            item_size: item.size,
            bytes_done: written,
            total_size,
            total_done: total_done.load(Ordering::Relaxed),
            status: TransferStatus::ItemDone,
            error: None,
        });
    }

    // Wait briefly for server's AllDone, but don't block forever.
    let _ =
        tokio::time::timeout(Duration::from_secs(30), read_msg::<serde_json::Value>(&mut ctrl_recv))
            .await;
    let _ = ctrl_send.finish();

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
        total_done: total_done.load(Ordering::Relaxed),
        status: TransferStatus::AllDone,
        error: None,
    });

    conn.close(0u32.into(), b"done");
    // Give in-flight close packets a chance.
    let _ = tokio::time::timeout(Duration::from_secs(2), endpoint.wait_idle()).await;
    Ok(())
}
