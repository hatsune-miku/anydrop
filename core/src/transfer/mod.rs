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
use tokio_util::sync::CancellationToken;

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
    /// Sender side: offer sent, waiting for the peer to accept or reject (no
    /// bytes are flowing yet).
    AwaitingAccept,
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
    /// User cancelled — terminal, no resume.
    Cancelled,
    /// User paused — sender stopped; receiver state preserved for `Resume`.
    Paused,
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
/// Fired when a peer asks this node (the *sender* of `transfer_id`) to
/// reconnect and resume a transfer the peer had paused. The host typically
/// wires this to `ServerHandle::resume_transfer(transfer_id)`.
pub type OnResumeRequest = Arc<dyn Fn(u64) + Send + Sync + 'static>;

/// Shared state used by the server to bridge async-await onto the host's
/// synchronous accept/reject API.
#[derive(Default)]
pub(crate) struct PendingOffers {
    /// transfer_id → oneshot sender filled in by `ServerHandle::respond`.
    pub map: Mutex<HashMap<u64, oneshot::Sender<Decision>>>,
}

/// Stored args for a send so it can be resumed verbatim after pause.
#[derive(Clone)]
pub(crate) struct SendArgs {
    pub target: SocketAddr,
    pub paths: Vec<PathBuf>,
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
    /// CancellationToken per in-flight transfer, keyed by `transfer_id`.
    /// Holds tokens for **both** sender-side and receiver-side active
    /// transfers — they don't collide because transfer_id is globally unique
    /// per send and a single host is rarely both sides of the same id.
    pub(crate) cancels: Arc<Mutex<HashMap<u64, CancellationToken>>>,
    /// Pause-token registry, kept SEPARATE from `cancels` so the affected task
    /// can tell a pause apart from a terminal cancel. Firing this token makes
    /// the sender/receiver take the "preserve state, report Paused" path rather
    /// than the "error/abort, destroy state" path. Like `cancels`, it holds
    /// tokens for both sender-side and receiver-side active transfers.
    pub(crate) pauses: Arc<Mutex<HashMap<u64, CancellationToken>>>,
    /// Send args parked for potential resume. Populated by `send_paths` and
    /// drained on cancel/AllDone. `resume_transfer` re-fires send with these.
    pub(crate) send_args: Arc<Mutex<HashMap<u64, SendArgs>>>,
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

    /// Send one or more files/folders to `target`.
    pub fn send_paths(&self, target: SocketAddr, paths: Vec<PathBuf>) -> u64 {
        let transfer_id: u64 = rand::random();
        self.send_with_id(transfer_id, target, paths);
        transfer_id
    }

    /// Internal: spawn a send using a caller-chosen `transfer_id`. Used by
    /// `send_paths` (fresh id) and `resume_transfer` (existing id, server
    /// recognizes and resumes from saved offsets).
    fn send_with_id(&self, transfer_id: u64, target: SocketAddr, paths: Vec<PathBuf>) {
        let token = CancellationToken::new();
        let pause = CancellationToken::new();
        self.cancels
            .lock()
            .unwrap()
            .insert(transfer_id, token.clone());
        self.pauses
            .lock()
            .unwrap()
            .insert(transfer_id, pause.clone());
        self.send_args.lock().unwrap().insert(
            transfer_id,
            SendArgs {
                target,
                paths: paths.clone(),
            },
        );

        let on_progress = self.on_progress.clone();
        let display_name = self.display_name.clone();
        let cancels = self.cancels.clone();
        let pauses = self.pauses.clone();
        let send_args = self.send_args.clone();
        self.runtime.spawn(async move {
            let result = client::send_paths_impl(
                transfer_id,
                target,
                paths,
                display_name,
                on_progress.clone(),
                token,
                pause,
            )
            .await;
            // The send task has ended (done, paused, cancelled, or errored).
            // Tear down the per-transfer cancel/pause tokens. `send_args` is
            // kept across pause so `resume_transfer` can re-fire; it is only
            // dropped on a hard error here (cancel/AllDone drop it in their
            // own handlers).
            cancels.lock().unwrap().remove(&transfer_id);
            pauses.lock().unwrap().remove(&transfer_id);
            if let Err(e) = result {
                send_args.lock().unwrap().remove(&transfer_id);
                on_progress(ProgressUpdate {
                    transfer_id,
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

    /// Cancel an in-flight transfer (sender or receiver, whichever side this
    /// handle is). Fires the cancellation token; the affected task surfaces
    /// `TransferStatus::Cancelled`. Send-side also notifies the peer via
    /// `Status::Abort` so the other UI doesn't dangle.
    ///
    /// Returns `true` if a transfer with that id was found and signalled.
    pub fn cancel_transfer(&self, transfer_id: u64) -> bool {
        let token = self.cancels.lock().unwrap().remove(&transfer_id);
        // Drop the pause token too — cancel is terminal, no resume.
        self.pauses.lock().unwrap().remove(&transfer_id);
        self.send_args.lock().unwrap().remove(&transfer_id);
        if let Some(t) = token {
            t.cancel();
            true
        } else {
            false
        }
    }

    /// Pause an in-flight transfer.  Fires the dedicated PAUSE token (NOT the
    /// cancel token), so the affected task takes the "preserve state, report
    /// Paused" path: it closes the QUIC connection with [`protocol::CLOSE_PAUSE`]
    /// so the peer can tell pause apart from a drop/cancel, emits
    /// `TransferStatus::Paused`, and keeps resume state alive.
    ///
    /// `send_args` is kept around so `resume_transfer` can re-fire with the
    /// original target / paths and the same transfer_id, which the receiver
    /// recognises via its active-transfer map (the already-received offsets
    /// get reported back in HelloAck). We also keep the cancel-token entry so
    /// resume's get-or-insert behaviour stays consistent; the spawned task
    /// itself tears both tokens down when it ends.
    pub fn pause_transfer(&self, transfer_id: u64) -> bool {
        let token = self.pauses.lock().unwrap().get(&transfer_id).cloned();
        // Note: do NOT remove send_args — that's what resume needs.
        if let Some(t) = token {
            t.cancel();
            true
        } else {
            false
        }
    }

    /// Resume a previously paused transfer.  Looks up the original send args
    /// and fires another `send_paths_impl` with the SAME transfer_id, which
    /// the receiver's resume path picks up automatically.
    pub fn resume_transfer(&self, transfer_id: u64) -> bool {
        let args = self.send_args.lock().unwrap().get(&transfer_id).cloned();
        if let Some(SendArgs { target, paths }) = args {
            self.send_with_id(transfer_id, target, paths);
            true
        } else {
            false
        }
    }

    /// Ask the *remote sender* of `transfer_id` to reconnect and resume.
    ///
    /// Used by the RECEIVER side: when the receiver paused a transfer it has
    /// no `send_args` of its own (only the sender does), so it cannot reconnect
    /// directly. Instead it opens a brief QUIC connection to `target` (the
    /// sender's transfer server) and sends a `ClientIntro::Resume` request,
    /// which fires the sender's `on_resume_request` callback (typically wired
    /// to the sender's own `resume_transfer`). Fire-and-forget on the runtime,
    /// like `send_paths`.
    pub fn request_remote_resume(&self, target: SocketAddr, transfer_id: u64) {
        self.runtime.spawn(async move {
            client::request_resume_impl(target, transfer_id).await;
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
/// `on_resume_request` is fired when a peer asks this node (the *sender* of a
/// transfer) to reconnect and resume; wire it to
/// [`ServerHandle::resume_transfer`].
pub fn start_server<O, P, R>(
    bind_addr: SocketAddr,
    display_name: String,
    on_offer: O,
    on_progress: P,
    on_resume_request: R,
) -> Result<ServerHandle, String>
where
    O: Fn(TransferOffer) + Send + Sync + 'static,
    P: Fn(ProgressUpdate) + Send + Sync + 'static,
    R: Fn(u64) + Send + Sync + 'static,
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
    let on_resume_request: OnResumeRequest = Arc::new(on_resume_request);
    let cancels: Arc<Mutex<HashMap<u64, CancellationToken>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let pauses: Arc<Mutex<HashMap<u64, CancellationToken>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let send_args: Arc<Mutex<HashMap<u64, SendArgs>>> = Arc::new(Mutex::new(HashMap::new()));

    {
        let endpoint = endpoint.clone();
        let pending = pending.clone();
        let on_offer = on_offer.clone();
        let on_progress = on_progress.clone();
        let on_resume_request = on_resume_request.clone();
        let cancels = cancels.clone();
        let pauses = pauses.clone();
        runtime.spawn(async move {
            server::run(
                endpoint,
                pending,
                on_offer,
                on_progress,
                on_resume_request,
                cancels,
                pauses,
            )
            .await;
        });
    }

    Ok(ServerHandle {
        runtime,
        endpoint,
        pending,
        on_progress,
        local_addr,
        display_name,
        cancels,
        pauses,
        send_args,
    })
}
