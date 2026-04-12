mod publisher;
mod receiver;

use std::net::{Ipv4Addr, SocketAddr};

use clap::Parser;
use log::info;
use tokio::sync::mpsc;
use zenoh::Config;

/// Bridge that receives raw ASTERIX (Eurocontrol) frames over UDP or TCP and
/// publishes them to a Zenoh session so that `zenohd` and other subscribers
/// can consume them.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Network address and port to listen on (e.g. 0.0.0.0:30192)
    #[arg(short, long, default_value = "0.0.0.0:30192")]
    listen_addr: SocketAddr,

    /// Transport protocol: "udp" or "tcp"
    #[arg(short, long, default_value = "udp", value_parser = parse_protocol)]
    protocol: Protocol,

    /// Multicast group to join (UDP only, e.g. 239.255.0.1).
    /// Ignored when --protocol=tcp.
    #[arg(short = 'm', long)]
    multicast_group: Option<String>,

    /// Zenoh router endpoint to connect to (e.g. tcp/127.0.0.1:7447).
    /// Leave empty to operate as a Zenoh peer without a dedicated router.
    #[arg(short, long)]
    zenoh_endpoint: Option<String>,

    /// Zenoh key expression under which frames are published.
    #[arg(short, long, default_value = "asterix/raw")]
    key_expr: String,

    /// Print category, SAC/SIC and frame length for every received frame.
    #[arg(short, long)]
    decode: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Protocol {
    Udp,
    Tcp,
}

fn parse_protocol(s: &str) -> Result<Protocol, String> {
    match s.to_ascii_lowercase().as_str() {
        "udp" => Ok(Protocol::Udp),
        "tcp" => Ok(Protocol::Tcp),
        other => Err(format!("unknown protocol '{}', use 'udp' or 'tcp'", other)),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    // Build Zenoh configuration.
    let mut config = Config::default();
    if let Some(ref endpoint) = args.zenoh_endpoint {
        config
            .insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
            .map_err(|e| anyhow::anyhow!("Failed to set Zenoh endpoint '{}': {:?}", endpoint, e))?;
    }

    info!("Opening Zenoh session…");
    let session = zenoh::open(config).await.map_err(|e| anyhow::anyhow!("{}", e))?;
    info!("Zenoh session open, publishing on '{}'", args.key_expr);

    // Channel between receiver tasks and the publisher loop.
    // 1024-frame buffer; if the publisher can't keep up, UDP frames are dropped
    // (they arrive at their own pace anyway) but TCP frames will slow the sender.
    let (tx, mut rx) = mpsc::channel::<bytes::Bytes>(1024);

    let key_expr = args.key_expr.clone();
    let decode = args.decode;

    // Spawn the network receiver.
    match args.protocol {
        Protocol::Udp => {
            let multicast: Option<Ipv4Addr> = args
                .multicast_group
                .as_deref()
                .map(receiver::udp::parse_multicast)
                .transpose()?;

            tokio::spawn(async move {
                if let Err(e) = receiver::udp::run(args.listen_addr, multicast, tx).await {
                    log::error!("UDP receiver error: {}", e);
                }
            });
        }
        Protocol::Tcp => {
            tokio::spawn(async move {
                if let Err(e) = receiver::tcp::run(args.listen_addr, tx).await {
                    log::error!("TCP receiver error: {}", e);
                }
            });
        }
    }

    // Publisher loop: drain the channel and publish to Zenoh.
    while let Some(frame) = rx.recv().await {
        if decode {
            log_asterix_frame(&frame);
        }
        publisher::publish_frame(&session, &key_expr, frame).await;
    }

    Ok(())
}

/// Log basic ASTERIX header fields without full decoding.
///
/// ASTERIX frame layout:
///   byte 0      : Category (CAT)
///   bytes 1-2   : Total length (big-endian, includes header)
///   bytes 3-4   : FSPEC (field specification, variable length)
///   bytes 5-6   : Data Source Identifier (DSI) — SAC (byte 5), SIC (byte 6)
fn log_asterix_frame(frame: &bytes::Bytes) {
    if frame.len() < 3 {
        log::debug!("ASTERIX frame too short to decode ({} bytes)", frame.len());
        return;
    }
    let cat = frame[0];
    let len = u16::from_be_bytes([frame[1], frame[2]]);

    if frame.len() >= 7 {
        // DSI is the first data item in most ASTERIX categories and sits
        // right after the FSPEC.  In CAT 048 / 062 / etc. the first FSPEC
        // byte is at index 3; DSI starts at index 4 (1-byte FSPEC min) or
        // later.  For a quick diagnostic we simply show bytes 3-6.
        let sac = frame[5];
        let sic = frame[6];
        log::info!(
            "[ASTERIX] CAT{:03} | len={:5} | SAC={:3} SIC={:3}",
            cat, len, sac, sic
        );
    } else {
        log::info!("[ASTERIX] CAT{:03} | len={:5}", cat, len);
    }
}
