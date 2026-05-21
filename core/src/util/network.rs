//! LAN reachability helpers.
//!
//! Picking the "right" IP for a peer that advertises several (typical when
//! discovery sees a machine on Wi-Fi, Ethernet, VPN, and a stack of virtual
//! NICs all at once) is surprisingly fiddly. Our strategy:
//!
//!   1. **Same-subnet first.** Compare each candidate against our own
//!      interface IP+netmask list. If the candidate sits inside one of our
//!      subnets, it's almost certainly reachable directly at layer 2 — and
//!      this happens to filter out Docker/VPN/Hyper-V virtual IPs as a side
//!      effect, since the remote machine's virtual NIC won't share a subnet
//!      with ours.
//!   2. **TCP probe fallback.** When no candidate matches our subnets, try
//!      a short connect to each in order; first that completes wins.
//!   3. **Last resort.** If all probes fail, hand back the first candidate
//!      anyway so the caller still gets a swing at it — better than nothing.

use if_addrs::{get_if_addrs, IfAddr};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

/// Re-order the input so same-subnet candidates come first, others after.
/// Relative order within each bucket is preserved.
pub fn rank_peer_ips(candidates: &[Ipv4Addr]) -> Vec<Ipv4Addr> {
    let locals = local_v4_with_mask();
    let mut same_subnet = Vec::new();
    let mut others = Vec::new();
    for ip in candidates {
        if locals
            .iter()
            .any(|(l, m)| in_same_subnet(*ip, *l, *m))
        {
            same_subnet.push(*ip);
        } else {
            others.push(*ip);
        }
    }
    same_subnet.extend(others);
    same_subnet
}

/// Try each candidate in order; first one that accepts a TCP connect on
/// `port` within `probe_timeout` wins. Probes are sequential — at LAN
/// latency this is fast enough, and keeps the code obvious.
pub fn first_reachable(
    candidates: &[Ipv4Addr],
    port: u16,
    probe_timeout: Duration,
) -> Option<Ipv4Addr> {
    candidates.iter().copied().find(|ip| {
        let addr = SocketAddr::new(IpAddr::V4(*ip), port);
        TcpStream::connect_timeout(&addr, probe_timeout).is_ok()
    })
}

/// One-shot helper combining rank + probe + last-resort fallback. Returns
/// `None` only when `candidates` is empty.
pub fn pick_best_peer(
    candidates: &[Ipv4Addr],
    port: u16,
    probe_timeout: Duration,
) -> Option<SocketAddr> {
    if candidates.is_empty() {
        return None;
    }
    let ranked = rank_peer_ips(candidates);
    if let Some(ip) = first_reachable(&ranked, port, probe_timeout) {
        return Some(SocketAddr::new(IpAddr::V4(ip), port));
    }
    // Nothing answered our probe, but the caller still wants a swing — hand
    // back the highest-ranked candidate.
    ranked
        .first()
        .map(|ip| SocketAddr::new(IpAddr::V4(*ip), port))
}

/// All non-loopback IPv4 interfaces on this host, paired with their netmask.
fn local_v4_with_mask() -> Vec<(Ipv4Addr, Ipv4Addr)> {
    get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.addr {
            IfAddr::V4(v4) => Some((v4.ip, v4.netmask)),
            _ => None,
        })
        .collect()
}

fn in_same_subnet(a: Ipv4Addr, b: Ipv4Addr, mask: Ipv4Addr) -> bool {
    let to_u = |ip: Ipv4Addr| u32::from(ip);
    (to_u(a) & to_u(mask)) == (to_u(b) & to_u(mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_subnet_basic_v4() {
        let mask = Ipv4Addr::new(255, 255, 255, 0);
        assert!(in_same_subnet(
            Ipv4Addr::new(192, 168, 1, 50),
            Ipv4Addr::new(192, 168, 1, 123),
            mask,
        ));
        assert!(!in_same_subnet(
            Ipv4Addr::new(192, 168, 1, 50),
            Ipv4Addr::new(192, 168, 2, 1),
            mask,
        ));
    }

    #[test]
    fn rank_preserves_order_within_buckets() {
        // Without local interfaces, every candidate ends up in `others` in
        // the original order — exercises the trivial fallback path.
        let cands = vec![
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(10, 0, 0, 2),
            Ipv4Addr::new(10, 0, 0, 3),
        ];
        let ranked = rank_peer_ips(&cands);
        // The function may still match against the runner's real
        // interfaces; we just assert no IP is dropped.
        assert_eq!(ranked.len(), cands.len());
        for ip in &cands {
            assert!(ranked.contains(ip));
        }
    }
}
