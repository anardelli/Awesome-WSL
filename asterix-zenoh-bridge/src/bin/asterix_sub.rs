/// asterix-sub — minimal Zenoh subscriber that prints received ASTERIX frames.
///
/// Useful for testing the bridge pipeline without a real ASTERIX client:
///   cargo run --bin asterix-sub -- --key-expr "asterix/raw"
use clap::Parser;
use zenoh::Config;

/// Subscriber that prints every raw ASTERIX frame received from Zenoh.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Zenoh key expression to subscribe to.
    #[arg(short, long, default_value = "asterix/raw")]
    key_expr: String,

    /// Zenoh router endpoint to connect to (e.g. tcp/127.0.0.1:7447).
    /// Leave empty to operate as a peer.
    #[arg(short, long)]
    zenoh_endpoint: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    let mut config = Config::default();
    if let Some(ref endpoint) = args.zenoh_endpoint {
        config
            .insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
            .map_err(|e| anyhow::anyhow!("Failed to set Zenoh endpoint '{}': {:?}", endpoint, e))?;
    }

    println!("Opening Zenoh session…");
    let session = zenoh::open(config).await.map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("Subscribing on '{}'…", args.key_expr);
    let subscriber = session
        .declare_subscriber(&args.key_expr)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("Waiting for frames (Ctrl-C to quit)…");
    while let Ok(sample) = subscriber.recv_async().await {
        let payload = sample.payload().to_bytes();
        let len = payload.len();

        if len >= 3 {
            let cat = payload[0];
            let declared_len = u16::from_be_bytes([payload[1], payload[2]]);
            print!(
                "[asterix-sub] key='{}' CAT{:03} declared_len={} received_bytes={}  hex=",
                sample.key_expr().as_str(),
                cat,
                declared_len,
                len,
            );
            // Print at most the first 16 bytes as hex for readability.
            for byte in payload.iter().take(16) {
                print!("{:02X} ", byte);
            }
            if len > 16 {
                print!("…");
            }
            println!();
        } else {
            println!(
                "[asterix-sub] key='{}' {} bytes (too short for ASTERIX header)",
                sample.key_expr().as_str(),
                len
            );
        }
    }

    Ok(())
}
