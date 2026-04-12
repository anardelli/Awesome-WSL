use std::net::SocketAddr;

use bytes::Bytes;
use log::{error, info, warn};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::Sender;

/// Accept connections on `listen_addr` and forward ASTERIX frames via `tx`.
///
/// Each connected client is handled in its own Tokio task.  The function
/// itself only returns on fatal errors (e.g. bind failure).
pub async fn run(listen_addr: SocketAddr, tx: Sender<Bytes>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    info!("TCP receiver listening on {}", listen_addr);

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                info!("TCP: accepted connection from {}", peer);
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, peer, tx_clone).await {
                        error!("TCP client {} error: {}", peer, e);
                    }
                });
            }
            Err(e) => {
                error!("TCP accept error: {}", e);
            }
        }
    }
}

/// Read length-prefixed ASTERIX frames from a TCP stream.
///
/// ASTERIX framing over TCP:
///   byte 0   : category (CAT)
///   bytes 1-2: total length in bytes (big-endian, includes the 3-byte header)
///   bytes 3..: data records
async fn handle_client(
    mut stream: TcpStream,
    peer: SocketAddr,
    tx: Sender<Bytes>,
) -> anyhow::Result<()> {
    loop {
        // Read the 3-byte ASTERIX header.
        let mut header = [0u8; 3];
        match stream.read_exact(&mut header).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                info!("TCP: connection from {} closed", peer);
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }

        let category = header[0];
        let total_len = u16::from_be_bytes([header[1], header[2]]) as usize;

        if total_len < 3 {
            warn!(
                "TCP: CAT{} from {} — declared length {} is less than header size, skipping frame",
                category, peer, total_len
            );
            continue;
        }

        let body_len = total_len - 3;
        let mut frame_buf = vec![0u8; total_len];
        frame_buf[0] = header[0];
        frame_buf[1] = header[1];
        frame_buf[2] = header[2];

        if body_len > 0 {
            match stream.read_exact(&mut frame_buf[3..]).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    warn!(
                        "TCP: CAT{} from {} — EOF mid-frame (expected {} body bytes)",
                        category, peer, body_len
                    );
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            }
        }

        let frame = Bytes::from(frame_buf);
        if tx.send(frame).await.is_err() {
            // Publisher side dropped — stop.
            return Ok(());
        }
    }
}
