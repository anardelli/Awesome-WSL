# asterix-zenoh-bridge

A Rust bridge that receives raw **ASTERIX** (Eurocontrol) frames over **UDP** or **TCP** and publishes them to a [Zenoh](https://zenoh.io/) session, making them available to `zenohd` and any downstream Zenoh subscriber.

> **Linux-only scope** — this project was designed to run on Linux as per its requirements. The `SO_REUSEPORT` socket option used for multiple-instance support is conditionally compiled for Linux via `#[cfg(target_os = "linux")]`; the code would also compile on BSD/macOS, but those platforms are not tested or supported.

---

## Architecture

```
[ASTERIX Transmitter]
        │  UDP datagram / TCP stream (binary RAW)
        ▼
[asterix-zenoh-bridge]  ← this binary (tokio async, Rust)
        │  zenoh::put(key_expr, raw_bytes)
        ▼
[zenohd]  ← Zenoh router / broker
        │
        ├─► [Subscriber A]  ← ASTERIX decoder
        ├─► [Subscriber B]  ← storage / logging
        └─► [Subscriber C]  ← display / alerting
```

---

## Prerequisites

- Rust 1.75+ (`rustup update stable`)
- Linux (tested on Ubuntu 22.04 / Debian 12)
- A running `zenohd` instance (optional — the bridge can operate as a peer without one)

---

## Build

```bash
cd asterix-zenoh-bridge
cargo build --release
```

Binaries are produced at:
- `target/release/asterix-zenoh-bridge`
- `target/release/asterix-sub`

---

## Usage

### Bridge (publisher)

```
USAGE:
    asterix-zenoh-bridge [OPTIONS]

OPTIONS:
    -l, --listen-addr <ADDR>        Address to listen on [default: 0.0.0.0:30192]
    -p, --protocol <PROTO>          Transport: udp | tcp [default: udp]
    -m, --multicast-group <IP>      Multicast group to join (UDP only, e.g. 239.255.0.1)
    -z, --zenoh-endpoint <EP>       Zenoh router endpoint (e.g. tcp/127.0.0.1:7447)
    -k, --key-expr <KEY>            Zenoh key expression [default: asterix/raw]
    -d, --decode                    Print CAT / SAC / SIC for each frame
    -h, --help                      Print help
    -V, --version                   Print version
```

#### Examples

**UDP unicast, connect to a local zenohd:**
```bash
./asterix-zenoh-bridge \
  --listen-addr 0.0.0.0:30192 \
  --protocol udp \
  --zenoh-endpoint tcp/127.0.0.1:7447 \
  --key-expr asterix/raw
```

**UDP multicast (group 239.255.0.1):**
```bash
./asterix-zenoh-bridge \
  --listen-addr 0.0.0.0:30192 \
  --protocol udp \
  --multicast-group 239.255.0.1 \
  --zenoh-endpoint tcp/127.0.0.1:7447 \
  --key-expr asterix/radar/cat048
```

**TCP stream receiver:**
```bash
./asterix-zenoh-bridge \
  --listen-addr 0.0.0.0:30192 \
  --protocol tcp \
  --zenoh-endpoint tcp/127.0.0.1:7447 \
  --decode
```

**Peer mode (no zenohd):**
```bash
./asterix-zenoh-bridge --listen-addr 0.0.0.0:30192
```

Enable verbose logging:
```bash
RUST_LOG=debug ./asterix-zenoh-bridge ...
```

---

### Subscriber (validation tool)

`asterix-sub` subscribes to a key expression and prints every received frame:

```bash
./asterix-sub --key-expr "asterix/raw"

# Connect to a remote zenohd
./asterix-sub --key-expr "asterix/**" --zenoh-endpoint tcp/192.168.1.10:7447
```

---

## ASTERIX framing

### UDP
One UDP datagram = one ASTERIX message.  
Header: `CAT (1 byte) | LEN_HIGH | LEN_LOW` — total frame length in big-endian.

### TCP
TCP carries a byte stream; framing is extracted by reading the 3-byte header first and then reading exactly `LEN - 3` more bytes.

---

## Key expressions

| Key expression         | Suggested use                        |
|------------------------|--------------------------------------|
| `asterix/raw`          | All categories, unfiltered           |
| `asterix/raw/cat048`   | Radar target reports (CAT 048)       |
| `asterix/raw/cat021`   | ADS-B target reports (CAT 021)       |
| `asterix/raw/cat062`   | Tracked target reports (CAT 062)     |

---

## License

Licensed under either of [MIT](../LICENSE) or Apache-2.0.
