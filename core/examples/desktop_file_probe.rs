//! Native desktop receive integration probe, confined to the loopback address.
//! args: PORT FILE [pending]; use an isolated debug AnyDrop instance.
use anydrop::transfer::{self, TransferStatus};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let port: u16 = args.get(1).ok_or("port required")?.parse()?;
    let path = PathBuf::from(args.get(2).ok_or("file required")?);
    let pending = args.get(3).is_some_and(|s| s == "pending");
    let (tx, rx) = mpsc::channel();
    let server = transfer::start_server(
        "127.0.0.1:0".parse()?,
        "Native receive test".into(),
        |_| {},
        move |p| {
            let _ = tx.send(p);
        },
        |_| {},
    )?;
    let id = server.send_paths(format!("127.0.0.1:{port}").parse()?, vec![path]);
    let started = Instant::now();
    let mut awaiting = false;
    while started.elapsed() < Duration::from_secs(if pending { 3 } else { 15 }) {
        if let Ok(p) = rx.recv_timeout(Duration::from_millis(100)) {
            awaiting |= p.status == TransferStatus::AwaitingAccept;
            if pending
                && matches!(
                    p.status,
                    TransferStatus::InProgress | TransferStatus::ItemDone | TransferStatus::AllDone
                )
            {
                return Err("Auto-received while disabled".into());
            }
            if p.status == TransferStatus::Error {
                return Err(p.error.unwrap_or_default().into());
            }
            if p.status == TransferStatus::AllDone {
                println!("Native auto-receive completed: {} bytes", p.total_done);
                server.close();
                return Ok(());
            }
        }
    }
    server.cancel_transfer(id);
    server.close();
    if pending && awaiting {
        println!("Native auto-receive disabled: offer stayed pending, no file bytes sent");
        Ok(())
    } else {
        Err("Timed out waiting for native acceptance".into())
    }
}
