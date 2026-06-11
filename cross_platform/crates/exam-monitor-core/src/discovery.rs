use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// Datagram sent by clients looking for a server on the local network.
pub const DISCOVER_MESSAGE: &[u8] = b"discover";
/// Datagram sent by the server, either as a periodic beacon or as a direct
/// reply to a client `DISCOVER_MESSAGE` probe.
pub const SERVER_MESSAGE: &[u8] = b"server";

/// Broadcast targets for server beacons and client probes.
///
/// Computes the directed broadcast address of every non-loopback IPv4
/// interface (e.g. 192.168.0.255 on a 192.168.0.x/24 network) and always
/// includes the limited broadcast address 255.255.255.255 as a fallback.
pub fn discovery_targets(port: u16) -> Vec<SocketAddr> {
    let mut targets = vec![SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, port))];

    match if_addrs::get_if_addrs() {
        Ok(interfaces) => {
            for interface in interfaces {
                if interface.is_loopback() {
                    continue;
                }

                if let if_addrs::IfAddr::V4(v4) = interface.addr {
                    if let Some(broadcast) = v4.broadcast {
                        let target = SocketAddr::V4(SocketAddrV4::new(broadcast, port));
                        if !targets.contains(&target) {
                            targets.push(target);
                        }
                    }
                }
            }
        }
        Err(error) => {
            eprintln!("DISCOVERY: failed to enumerate network interfaces: {error}");
        }
    }

    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_always_include_limited_broadcast() {
        let targets = discovery_targets(4_321);
        assert!(targets
            .contains(&SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, 4_321))));
    }

    #[test]
    fn targets_use_requested_port_and_have_no_duplicates() {
        let targets = discovery_targets(1_234);
        assert!(targets.iter().all(|target| target.port() == 1_234));

        let unique: std::collections::HashSet<_> = targets.iter().collect();
        assert_eq!(targets.len(), unique.len());
    }

    #[test]
    fn targets_exclude_loopback_networks() {
        let targets = discovery_targets(1_234);
        assert!(targets
            .iter()
            .all(|target| !target.ip().is_loopback()));
    }
}
