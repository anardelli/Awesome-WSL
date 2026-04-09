#!/usr/bin/env python3
"""
asterix_zenoh_bridge.py
=======================
Bridge that receives raw Asterix binary frames (Eurocontrol surveillance data)
over UDP (unicast or multicast) and publishes each frame as raw bytes onto a
Zenoh key expression so that any zenohd subscriber can consume it.

Architecture:
    [Asterix transmitter (UDP)] ──► [this bridge] ──► [zenohd] ──► [subscribers]

Usage:
    python3 asterix_zenoh_bridge.py [OPTIONS]

See --help for all options.
"""

import argparse
import logging
import signal
import socket
import struct
import sys

import zenoh

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
log = logging.getLogger("asterix_zenoh_bridge")

# ---------------------------------------------------------------------------
# Defaults (aligned with common Eurocontrol ASTERIX network parameters)
# ---------------------------------------------------------------------------
DEFAULT_UDP_HOST = "0.0.0.0"       # listen on all interfaces
DEFAULT_UDP_PORT = 8600             # common ASTERIX UDP port
DEFAULT_ZENOH_KEY = "asterix/raw"   # Zenoh key expression
DEFAULT_BUF_SIZE = 65535            # max UDP datagram size


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def join_multicast_group(sock: socket.socket, group: str, iface: str = "") -> None:
    """Join a UDP multicast group on the given socket."""
    mreq = struct.pack("4s4s", socket.inet_aton(group),
                       socket.inet_aton(iface) if iface else b"\x00" * 4)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)
    log.info("Joined multicast group %s (iface=%s)", group, iface or "default")


def build_udp_socket(host: str, port: int,
                     multicast_group: str | None,
                     multicast_iface: str) -> socket.socket:
    """Create and bind a UDP socket, optionally joining a multicast group."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    # SO_REUSEPORT allows multiple processes to bind to the same port (Linux)
    if hasattr(socket, "SO_REUSEPORT"):
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)

    bind_addr = host if multicast_group is None else ""
    # Binding to all interfaces (0.0.0.0) is intentional: the bridge must
    # receive UDP datagrams from external Asterix radar transmitters on the
    # network.  Restrict traffic at the OS firewall level if needed.
    sock.bind((bind_addr, port))
    log.info("UDP socket bound to %s:%d", bind_addr or "0.0.0.0", port)

    if multicast_group:
        join_multicast_group(sock, multicast_group, multicast_iface)

    return sock


def extract_asterix_category(data: bytes) -> int | None:
    """
    Return the Asterix category byte (first byte of the ASTERIX record)
    or None if the frame is too short.
    """
    return data[0] if len(data) >= 3 else None


# ---------------------------------------------------------------------------
# Main bridge loop
# ---------------------------------------------------------------------------

def run_bridge(args: argparse.Namespace) -> None:
    # --- open Zenoh session -------------------------------------------------
    zenoh_cfg = zenoh.Config()
    if args.zenoh_connect:
        # Override the default locator (e.g. tcp/127.0.0.1:7447)
        zenoh_cfg.insert_json5(
            "connect/endpoints",
            f'["{args.zenoh_connect}"]',
        )

    log.info("Opening Zenoh session …")
    session = zenoh.open(zenoh_cfg)

    publisher = session.declare_publisher(
        args.zenoh_key,
        encoding=zenoh.Encoding.APP_OCTET_STREAM,
    )
    log.info("Zenoh publisher declared on key '%s'", args.zenoh_key)

    # --- open UDP socket ----------------------------------------------------
    sock = build_udp_socket(
        args.udp_host,
        args.udp_port,
        args.multicast_group,
        args.multicast_iface,
    )

    # --- graceful shutdown ---------------------------------------------------
    running = True

    def _stop(signum, _frame):  # noqa: ANN001
        nonlocal running
        log.info("Signal %d received – stopping bridge …", signum)
        running = False

    signal.signal(signal.SIGINT, _stop)
    signal.signal(signal.SIGTERM, _stop)

    # --- bridge loop --------------------------------------------------------
    log.info(
        "Bridge running. Listening UDP %s:%d → Zenoh '%s'",
        args.udp_host, args.udp_port, args.zenoh_key,
    )
    frames_total = 0
    sock.settimeout(1.0)   # allow the loop to check `running` every second

    while running:
        try:
            data, addr = sock.recvfrom(args.buf_size)
        except socket.timeout:
            continue
        except OSError as exc:
            if running:
                log.error("Socket error: %s", exc)
            break

        frames_total += 1
        cat = extract_asterix_category(data)

        if args.verbose:
            log.debug(
                "Frame #%d from %s:%d  cat=%s  len=%d bytes",
                frames_total, addr[0], addr[1],
                f"CAT{cat:03d}" if cat is not None else "?",
                len(data),
            )

        # Publish raw bytes to Zenoh
        publisher.put(data)

    # --- cleanup ------------------------------------------------------------
    log.info("Closing UDP socket and Zenoh session (frames forwarded: %d) …", frames_total)
    sock.close()
    publisher.undeclare()
    session.close()
    log.info("Bridge stopped.")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Forward raw Asterix UDP frames to a Zenoh publisher.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--udp-host", default=DEFAULT_UDP_HOST,
        help="Local IP address to bind the UDP socket to.",
    )
    parser.add_argument(
        "--udp-port", type=int, default=DEFAULT_UDP_PORT,
        help="UDP port to listen on.",
    )
    parser.add_argument(
        "--multicast-group", default=None, metavar="IP",
        help="Multicast group IP to join (e.g. 239.255.0.1).  "
             "Leave empty for unicast mode.",
    )
    parser.add_argument(
        "--multicast-iface", default="", metavar="IP",
        help="Local interface IP to use when joining the multicast group.",
    )
    parser.add_argument(
        "--buf-size", type=int, default=DEFAULT_BUF_SIZE,
        help="UDP receive buffer size in bytes.",
    )
    parser.add_argument(
        "--zenoh-key", default=DEFAULT_ZENOH_KEY,
        help="Zenoh key expression used to publish frames.",
    )
    parser.add_argument(
        "--zenoh-connect", default=None, metavar="LOCATOR",
        help="Zenoh endpoint locator to connect to "
             "(e.g. tcp/127.0.0.1:7447).  "
             "Defaults to auto-discovery.",
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true",
        help="Enable per-frame debug logging.",
    )
    return parser.parse_args(argv)


def main() -> None:
    args = parse_args()
    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)
    run_bridge(args)


if __name__ == "__main__":
    main()
