# Asterix ↔ Zenoh Bridge

Forward raw **Asterix** surveillance frames (Eurocontrol ATC data) from a UDP
transmitter to any **Zenoh** subscriber through a local `zenohd` router.

```
[Asterix transmitter (UDP/multicast)]
          │
          ▼
  asterix_zenoh_bridge.py          ← this bridge
          │
          ▼
       zenohd                      ← router (zenohd_config.json5)
          │
          ▼
  asterix_subscriber.py            ← example consumer
```

---

## Requirements

| Requirement | Notes |
|-------------|-------|
| Python 3.9+ | Available in all modern WSL2 distributions |
| `zenohd`    | Zenoh router binary ([releases](https://github.com/eclipse-zenoh/zenoh/releases)) |
| pip packages | See `requirements.txt` |

---

## Quick start

### 1 – Install Python dependencies

```bash
pip install -r requirements.txt
```

### 2 – Start the Zenoh router

Download the `zenohd` binary for your platform (Linux x86-64 / ARM64):

```bash
# Example – replace VERSION with the latest release tag
VERSION=1.0.0
wget https://github.com/eclipse-zenoh/zenoh/releases/download/${VERSION}/zenoh-${VERSION}-x86_64-unknown-linux-gnu.zip
unzip zenoh-${VERSION}-x86_64-unknown-linux-gnu.zip
./zenohd --config zenohd_config.json5
```

### 3 – Start the bridge

**Unicast (single source):**

```bash
python3 asterix_zenoh_bridge.py \
    --udp-host 0.0.0.0 \
    --udp-port 8600 \
    --zenoh-key asterix/raw
```

**Multicast group:**

```bash
python3 asterix_zenoh_bridge.py \
    --udp-port 8600 \
    --multicast-group 239.255.0.1 \
    --zenoh-key asterix/raw
```

### 4 – Start a subscriber (validation)

```bash
python3 asterix_subscriber.py --zenoh-key asterix/raw
```

If the `asterix` Python package is installed, each frame is decoded and printed.
Otherwise, a hex preview is shown.

---

## Bridge options

```
$ python3 asterix_zenoh_bridge.py --help

optional arguments:
  --udp-host IP         Local IP address to bind to.        [0.0.0.0]
  --udp-port PORT       UDP port to listen on.              [8600]
  --multicast-group IP  Multicast group IP to join.         [disabled]
  --multicast-iface IP  Local interface for multicast.      [default]
  --buf-size BYTES      UDP receive buffer size.            [65535]
  --zenoh-key KEY       Zenoh key expression.               [asterix/raw]
  --zenoh-connect LOC   Zenoh locator (e.g. tcp/127.0.0.1:7447). [auto]
  -v, --verbose         Per-frame debug output.
```

---

## WSL2 network notes

WSL2 uses a virtual network interface and does **not** receive Windows-side
multicast traffic by default.  Two workarounds:

### Option A – socat relay (Windows → WSL2)

Run this in a **Windows** PowerShell or CMD terminal:

```powershell
# Forward multicast traffic arriving on the Windows NIC into WSL2 UDP
socat UDP4-RECVFROM:8600,ip-add-membership=239.255.0.1:0.0.0.0,fork \
      UDP4-SENDTO:$(wsl hostname -I | awk '{print $1}'):8600
```

Then run the bridge in WSL2 in unicast mode (`--udp-host 0.0.0.0 --udp-port 8600`).

### Option B – run everything natively on Windows

Install Python for Windows, `zenohd.exe`, and run the bridge and subscriber
directly on Windows — no WSL2 network translation needed.

### Option C – WSL2 mirrored networking (Windows 11 22H2+)

Enable mirrored networking in `%USERPROFILE%\.wslconfig`:

```ini
[wsl2]
networkingMode=mirrored
```

Then multicast traffic is available inside WSL2 without any relay.

---

## Zenoh key expression layout

| Key expression         | Content |
|------------------------|---------|
| `asterix/raw`          | Any Asterix category, raw bytes |
| `asterix/raw/cat048`   | CAT 048 (Monoradar target reports) |
| `asterix/raw/cat062`   | CAT 062 (SDPS track messages) |

Publish to category-specific keys by filtering the first byte of each frame
and adjusting `--zenoh-key` accordingly, or by running one bridge instance
per category.

---

## References

- [Zenoh documentation](https://zenoh.io/docs/)
- [ASTERIX standard (Eurocontrol)](https://www.eurocontrol.int/asterix)
- [asterix Python package](https://pypi.org/project/asterix/)
- [CroatiaControl asterix parser (C++)](https://github.com/CroatiaControlLtd/asterix)
- [Wireshark Asterix plugin](https://www.wireshark.org/)
