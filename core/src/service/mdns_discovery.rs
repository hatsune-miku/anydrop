//! mDNS / DNS-SD discovery, complementing the legacy UDP broadcast scan.
//!
//! Both paths feed the **same** `peers` set and `last_seen` map — they
//! independently insert into a `HashSet<Peer>` keyed by `(host, port)`, so a
//! peer reachable on both channels is naturally deduplicated.
//!
//! Why have both:
//! * UDP broadcast is fast and simple but limited to the same broadcast
//!   domain and gets dropped by routers / some access points with strict
//!   client isolation.
//! * mDNS uses link-local multicast (224.0.0.251 / FF02::FB on udp/5353),
//!   which most home/office wifi networks explicitly permit. It also makes
//!   AnyDrop visible to standard Bonjour browsers (`dns-sd -B`, avahi-browse,
//!   macOS Finder, etc.) which is handy for debugging.
//!
//! Running both gives us a strict superset of either alone's coverage.

use crate::network::peer::Peer;
use crate::service::discovery_service::PeerCollectionType;
use log::{info, warn};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// DNS-SD service type for AnyDrop instances on the local link.
pub const SERVICE_TYPE: &str = "_anydrop._udp.local.";

/// Shape of the `last_seen` map the host application maintains for TTL-based
/// peer expiry. Matches what the UDP `DiscoveryService` writes into.
pub type LastSeenMap = Arc<Mutex<HashMap<String, Instant>>>;

/// Run the mDNS responder + browser until `should_interrupt` returns `true`.
///
/// Blocks the calling thread. Intended to be run from a dedicated worker.
pub fn run(
    peers: PeerCollectionType,
    last_seen: Option<LastSeenMap>,
    data_port: u16,
    group_id: u32,
    display_name: String,
    should_interrupt: Box<dyn Fn() -> bool + Send>,
) -> Result<(), String> {
    let daemon = ServiceDaemon::new().map_err(|e| format!("mdns daemon: {}", e))?;

    // .local. hostname for the A/AAAA records the daemon publishes.
    let os_host = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "anydrop".to_string());
    let host_name = format!("{}.local.", trim_dots(&os_host));

    // TXT records: tiny, ASCII, lowercase keys per RFC 6763 §6.4.
    let mut props: HashMap<String, String> = HashMap::new();
    props.insert("v".into(), env!("CARGO_PKG_VERSION").into());
    props.insert("g".into(), group_id.to_string());
    props.insert("dn".into(), display_name.clone());

    let instance = sanitize_instance(&display_name);

    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &host_name,
        "", // empty: daemon auto-enumerates non-loopback interface IPs
        data_port,
        Some(props),
    )
    .map_err(|e| format!("mdns info: {}", e))?
    .enable_addr_auto();

    if let Err(e) = daemon.register(info) {
        warn!("mdns: register failed: {}", e);
    } else {
        info!(
            "mdns: registered '{}' on '{}' port {} (group {})",
            instance, host_name, data_port, group_id
        );
    }

    let rx = daemon
        .browse(SERVICE_TYPE)
        .map_err(|e| format!("mdns browse: {}", e))?;

    // Drain events until we're told to stop. We use try_recv + a short sleep
    // so the loop stays responsive to shutdown without blocking on a channel
    // wait the daemon never satisfies during quiet periods.
    while !should_interrupt() {
        let mut drained = false;
        while let Ok(ev) = rx.try_recv() {
            drained = true;
            match ev {
                ServiceEvent::ServiceResolved(svc) => {
                    handle_resolved(svc, group_id, &peers, last_seen.as_ref());
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    // We don't keep a fullname → peer map, so we don't actively
                    // prune here — the host's TTL sweep will catch removed
                    // peers within seconds once mDNS stops refreshing them.
                    info!("mdns: service removed: {}", fullname);
                }
                _ => {}
            }
        }
        if !drained {
            thread::sleep(Duration::from_millis(200));
        }
    }

    let _ = daemon.shutdown();
    info!("mdns: discovery loop stopped");
    Ok(())
}

/// Apply a resolved mDNS service to our shared peer set.
fn handle_resolved(
    svc: mdns_sd::ServiceInfo,
    expected_group: u32,
    peers: &PeerCollectionType,
    last_seen: Option<&LastSeenMap>,
) {
    // Group filter — silently drop peers that advertise a different group_id.
    let group: u32 = svc
        .get_property_val_str("g")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if group != expected_group {
        return;
    }

    let display = svc
        .get_property_val_str("dn")
        .map(str::to_string)
        .unwrap_or_else(|| svc.get_fullname().to_string());
    let port = svc.get_port();
    let addrs = svc.get_addresses().clone();

    // Filter ourselves: if the announcement resolves to any of our own
    // interface IPs, it's our own broadcast bouncing back.
    let mine = local_ipv4_set();
    if addrs.iter().any(|a| matches!(a, IpAddr::V4(v) if mine.contains(v))) {
        return;
    }

    let now = Instant::now();
    let mut peer_set = match peers.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    for addr in addrs {
        let IpAddr::V4(v4) = addr else {
            continue; // IPv6 elided for now — the rest of the stack is v4
        };
        let peer = Peer::from(&v4, port, Some(&display));
        let host_key = peer.host().clone();
        peer_set.replace(peer);
        if let Some(ls) = last_seen {
            if let Ok(mut m) = ls.lock() {
                m.insert(host_key, now);
            }
        }
    }
}

/// Snapshot of our own IPv4 interface addresses, used to skip self-resolves.
fn local_ipv4_set() -> HashSet<std::net::Ipv4Addr> {
    local_ip_address::list_afinet_netifas()
        .map(|v| {
            v.into_iter()
                .filter_map(|(_, ip)| match ip {
                    IpAddr::V4(v4) => Some(v4),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn trim_dots(s: &str) -> String {
    s.trim_end_matches(".local")
        .trim_end_matches('.')
        .to_string()
}

/// DNS-SD instance names go into a single DNS label, so dots and slashes are
/// not legal. Replace anything we can't trust with '-'; mdns-sd handles the
/// uniqueness-suffix dance if multiple peers pick the same display name.
fn sanitize_instance(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '.' | '/' | '\0' => '-',
            _ => c,
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "AnyDrop".to_string()
    } else {
        trimmed.to_string()
    }
}
