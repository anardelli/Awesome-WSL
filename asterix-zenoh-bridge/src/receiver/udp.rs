use std::net::{Ipv4Addr, SocketAddr};

use bytes::Bytes;
use log::{error, info, warn};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::Sender;

/// Maximum UDP datagram size we expect to receive.
const MAX_DATAGRAM_SIZE: usize = 65_535;

/// Listen on `listen_addr` for ASTERIX datagrams.
///
/// If `multicast_group` is set the socket will join that multicast group on
/// the interface inferred from `listen_addr`.  Every well-formed ASTERIX frame
/// is forwarded through `tx`.
pub async fn run(
    listen_addr: SocketAddr,
    multicast_group: Option<Ipv4Addr>,
    tx: Sender<Bytes>,
) -> anyhow::Result<()> {
    let socket = build_socket(listen_addr, multicast_group)?;
    let std_socket: std::net::UdpSocket = socket.into();
    std_socket.set_nonblocking(true)?;
    let udp = UdpSocket::from_std(std_socket)?;

    info!("UDP receiver listening on {}", listen_addr);

    let mut buf = vec![0u8; MAX_DATAGRAM_SIZE];
    loop {
        match udp.recv_from(&mut buf).await {
            Ok((n, peer)) => {
                if n < 3 {
                    warn!("UDP: received {} bytes from {} — too short for ASTERIX header, skipping", n, peer);
                    continue;
                }
                let category = buf[0];
                let declared_len = u16::from_be_bytes([buf[1], buf[2]]) as usize;
                if declared_len != n {
                    warn!(
                        "UDP: CAT{} from {} — declared length {} differs from received {} bytes",
                        category, peer, declared_len, n
                    );
                    // We still forward the raw bytes as received; the subscriber
                    // can decide whether to discard or handle partial frames.
                }
                let frame = Bytes::copy_from_slice(&buf[..n]);
                if tx.send(frame).await.is_err() {
                    // Receiver dropped — time to stop.
                    break;
                }
            }
            Err(e) => {
                error!("UDP recv error: {}", e);
            }
        }
    }
    Ok(())
}

/// Build a socket with `SO_REUSEADDR` (and `SO_REUSEPORT` on Linux) so that
/// multiple instances can bind to the same port.  Optionally joins a multicast
/// group.
fn build_socket(
    bind_addr: SocketAddr,
    multicast_group: Option<Ipv4Addr>,
) -> anyhow::Result<Socket> {
    let domain = if bind_addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;

    // SO_REUSEPORT is Linux-only
    #[cfg(target_os = "linux")]
    socket.set_reuse_port(true)?;

    socket.bind(&bind_addr.into())?;

    if let Some(group) = multicast_group {
        let interface = match bind_addr {
            SocketAddr::V4(v4) => *v4.ip(),
            SocketAddr::V6(_) => Ipv4Addr::UNSPECIFIED,
        };
        socket.join_multicast_v4(&group, &interface)?;
        info!("Joined multicast group {} on interface {}", group, interface);
    }

    Ok(socket)
}

/// Helper: parse an optional multicast group address from a CLI string.
pub fn parse_multicast(s: &str) -> anyhow::Result<Ipv4Addr> {
    let addr: Ipv4Addr = s.parse()?;
    if !addr.is_multicast() {
        anyhow::bail!("{} is not a multicast address", s);
    }
    Ok(addr)
}
