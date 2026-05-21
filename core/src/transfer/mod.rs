//! transfer_v2 — QUIC-based file/folder transfer.
//!
//! Public surface:
//!   * [`start_server`] – open a QUIC endpoint that receives transfers.
//!   * [`ServerHandle::respond`] – accept or reject a pending offer.
//!   * [`ServerHandle::send_paths`] – send one or more files/folders to a peer.
//!
//! Everything is driven from a single internal tokio runtime owned by the
//! [`ServerHandle`]. Callers from sync code call these methods directly; they
//! return without blocking on the actual transfer.

pub mod cert;
pub mod client;
pub mod protocol;
pub mod server;
pub mod walk;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

pub use protocol::Item;

/// Direction of a progress update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Send,
    Recv,
}

/// Coarse status of a transfer for UI display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferStatus {
    /// Offer received; waiting on user decision.
    PendingDecision,
    /// Bytes flowing.
    InProgress,
    /// One item finished; more to go.
    ItemDone,
    /// All items finished successfully.
    AllDone,
    /// Local error or peer aborted.
    Error,
    /// Peer rejected the offer (sender side).
    Rejected,
}

/// Offer received by the server, surfaced to the host application via the
/// `on_offer` callback registered with [`start_server`].
#[derive(Clone, Debug)]
pub struct TransferOffer {
    pub transfer_id: u64,
    pub remote_addr: SocketAddr,
    pub display_name: String,
    pub items: Vec<Item>,
    /// Sum of `size` for non-directory items.
    pub total_size: u64,
}

/// Decision returned by the host application for a pending offer.
#[derive(Clone, Debug)]
pub enum Decision {
    Accept { save_root: PathBuf },
    Reject { reason: String },
}

/// Progress / status update fired by both server (receive side) and client
/// (send side).
#[derive(Clone, Debug)]
pub struct ProgressUpdate {
    pub transfer_id: u64,
    pub direction: Direction,
    pub remote_addr: SocketAddr,
    /// Peer's display name (best-effort; may be empty on send side).
    pub display_name: String,
    /// Index into the `items` vec from the original Hello.
    pub item_idx: u32,
    pub rel_path: String,
    pub item_size: u64,
    pub bytes_done: u64,
    pub total_size: u64,
    pub total_done: u64,
    pub status: TransferStatus,
    /// Error message when `status == Error`.
    pub error: Option<String>,
}

pub type OnOffer = Arc<dyn Fn(TransferOffer) + Send + Sync + 'static>;
pub type OnProgress = Arc<dyn Fn(ProgressUpdate) + Send + Sync + 'static>;

/// Shared state used by the server to bridge async-await onto the host's
/// synchronous accept/reject API.
#[derive(Default)]
pub(crate) struct PendingOffers {
    /// transfer_id → oneshot sender filled in by `ServerHandle::respond`.
    pub map: Mutex<HashMap<u64, oneshot::Sender<Decision>>>,
}

/// Handle returned by [`start_server`]. Owns the tokio runtime and the QUIC
/// endpoint; drop / [`ServerHandle::close`] tears everything down.
pub struct ServerHandle {
    runtime: Arc<tokio::runtime::Runtime>,
    endpoint: quinn::Endpoint,
    pending: Arc<PendingOffers>,
    on_progress: OnProgress,
    local_addr: SocketAddr,
    display_name: String,
}

impl ServerHandle {
    /// Local socket the QUIC server is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Respond to a pending offer. Safe to call from any thread.
    pub fn respond(&self, transfer_id: u64, decision: Decision) {
        let sender = self.pending.map.lock().unwrap().remove(&transfer_id);
        if let Some(tx) = sender {
            let _ = tx.send(decision);
        }
    }

    /// Send one or more files/folders to `target`. Runs the transfer on this
    /// handle's runtime; returns immediately.
    pub fn send_paths(&self, target: SocketAddr, paths: Vec<PathBuf>) {
        let on_progress = self.on_progress.clone();
        let display_name = self.display_name.clone();
        self.runtime.spawn(async move {
            if let Err(e) =
                client::send_paths_impl(target, paths, display_name, on_progress.clone()).await
            {
                on_progress(ProgressUpdate {
                    transfer_id: 0,
                    direction: Direction::Send,
                    remote_addr: target,
                    display_name: String::new(),
                    item_idx: 0,
                    rel_path: String::new(),
                    item_size: 0,
                    bytes_done: 0,
                    total_size: 0,
                    total_done: 0,
                    status: TransferStatus::Error,
                    error: Some(e),
                });
            }
        });
    }

    /// Gracefully close the endpoint and wait for outstanding tasks.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

/// Start the QUIC server on `bind_addr` and return a handle.
///
/// `on_offer` is invoked when a peer sends a `Hello`; the host application
/// must eventually call [`ServerHandle::respond`] with the same `transfer_id`.
/// `on_progress` is fired for both send- and receive-side updates.
pub fn start_server<O, P>(
    bind_addr: SocketAddr,
    display_name: String,
    on_offer: O,
    on_progress: P,
) -> Result<ServerHandle, String>
where
    O: Fn(TransferOffer) + Send + Sync + 'static,
    P: Fn(ProgressUpdate) + Send + Sync + 'static,
{
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("anydrop-transfer-v2")
            .build()
            .map_err(|e| format!("tokio runtime: {}", e))?,
    );

    let (cert, key) = cert::generate_self_signed()?;
    let server_cfg = cert::server_config(cert, key)?;
    let endpoint = {
        let _g = runtime.enter();
        quinn::Endpoint::server(server_cfg, bind_addr).map_err(|e| format!("endpoint: {}", e))?
    };
    let local_addr = endpoint.local_addr().map_err(|e| format!("local_addr: {}", e))?;

    let pending = Arc::new(PendingOffers::default());
    let on_offer: OnOffer = Arc::new(on_offer);
    let on_progress: OnProgress = Arc::new(on_progress);

    {
        let endpoint = endpoint.clone();
        let pending = pending.clone();
        let on_offer = on_offer.clone();
        let on_progress = on_progress.clone();
        runtime.spawn(async move {
            server::run(endpoint, pending, on_offer, on_progress).await;
        });
    }

    Ok(ServerHandle {
        runtime,
        endpoint,
        pending,
        on_progress,
        local_addr,
        display_name,
    })
}
