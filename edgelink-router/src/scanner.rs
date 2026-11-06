use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const RESPONSE_WINDOW: Duration = Duration::from_millis(350);

pub fn discover(addresses: &[(Ipv4Addr, u8)], port: u16) -> Result<Option<Ipv4Addr>, String> {
    let nonce = make_nonce();
    let query = crate::protocol::make_query(&nonce)
        .ok_or_else(|| "could not create discovery query".to_owned())?;

    for (local_address, prefix) in addresses {
        let Some(broadcast) = subnet_broadcast(*local_address, *prefix) else {
            continue;
        };
        let socket = match UdpSocket::bind(SocketAddrV4::new(*local_address, 0)) {
            Ok(socket) => socket,
            Err(error) => {
                eprintln!("cannot bind discovery socket on {local_address}: {error}");
                continue;
            }
        };
        socket
            .set_broadcast(true)
            .map_err(|error| format!("cannot enable UDP broadcast: {error}"))?;
        socket
            .set_read_timeout(Some(RESPONSE_WINDOW))
            .map_err(|error| format!("cannot set discovery timeout: {error}"))?;
        socket
            .send_to(query.as_bytes(), SocketAddrV4::new(broadcast, port))
            .map_err(|error| format!("cannot send discovery broadcast: {error}"))?;

        let deadline = Instant::now() + RESPONSE_WINDOW;
        let mut response = [0_u8; 128];
        loop {
            match socket.recv_from(&mut response) {
                Ok((length, peer)) => {
                    if !crate::protocol::response_matches(&response[..length], &nonce) {
                        continue;
                    }
                    if let SocketAddr::V4(peer) = peer {
                        if same_subnet(*local_address, *peer.ip(), *prefix) {
                            return Ok(Some(*peer.ip()));
                        }
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(error) => return Err(format!("discovery receive failed: {error}")),
            }
            if Instant::now() >= deadline {
                break;
            }
        }
    }
    Ok(None)
}

fn subnet_broadcast(address: Ipv4Addr, prefix: u8) -> Option<Ipv4Addr> {
    if !(1..=30).contains(&prefix) {
        return None;
    }
    let mask = u32::MAX << (32 - prefix);
    Some(Ipv4Addr::from(u32::from(address) | !mask))
}

fn same_subnet(left: Ipv4Addr, right: Ipv4Addr, prefix: u8) -> bool {
    let mask = match prefix {
        0 => 0,
        1..=31 => u32::MAX << (32 - prefix),
        32 => u32::MAX,
        _ => return false,
    };
    (u32::from(left) & mask) == (u32::from(right) & mask)
}

fn make_nonce() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{time:016x}{:08x}{counter:08x}", std::process::id())
}
