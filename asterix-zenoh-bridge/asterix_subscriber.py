#!/usr/bin/env python3
"""
asterix_subscriber.py
=====================
Example Zenoh subscriber that receives raw Asterix frames published by
asterix_zenoh_bridge.py and optionally decodes them with the `asterix` library.

Usage:
    python3 asterix_subscriber.py [OPTIONS]

See --help for all options.
"""

import argparse
import logging
import signal
import time

import zenoh

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
log = logging.getLogger("asterix_subscriber")

DEFAULT_ZENOH_KEY = "asterix/raw"


# ---------------------------------------------------------------------------
# Optional Asterix decoding
# ---------------------------------------------------------------------------

def _try_import_asterix():
    """Return the `asterix` module if available, else None."""
    try:
        import asterix  # noqa: PLC0415
        return asterix
    except ImportError:
        return None


def decode_frame(data: bytes, asterix_mod) -> str:
    """
    Return a human-readable description of the Asterix frame.
    Falls back to a hex dump when the `asterix` library is not installed or
    decoding fails.
    """
    if asterix_mod is None:
        return f"<raw {len(data)} bytes> hex={data[:32].hex()}{'…' if len(data) > 32 else ''}"

    try:
        records = asterix_mod.parse(data)
        lines = []
        for rec in records:
            lines.append(
                f"CAT{rec.get('category', '?'):03d}  "
                f"items={list(rec.keys())}"
            )
        return " | ".join(lines) if lines else "<empty parse result>"
    except Exception as exc:  # noqa: BLE001
        return f"<decode error: {exc}>  hex={data[:32].hex()}"


# ---------------------------------------------------------------------------
# Subscriber
# ---------------------------------------------------------------------------

def run_subscriber(args: argparse.Namespace) -> None:
    asterix_mod = _try_import_asterix()
    if asterix_mod:
        log.info("asterix library found – frames will be decoded.")
    else:
        log.info(
            "asterix library not found – showing raw hex. "
            "Install with: pip install asterix"
        )

    # Open Zenoh session
    zenoh_cfg = zenoh.Config()
    if args.zenoh_connect:
        zenoh_cfg.insert_json5(
            "connect/endpoints",
            f'["{args.zenoh_connect}"]',
        )

    log.info("Opening Zenoh session …")
    session = zenoh.open(zenoh_cfg)

    frame_count = 0

    def on_sample(sample: zenoh.Sample) -> None:
        nonlocal frame_count
        frame_count += 1
        payload = bytes(sample.payload)
        description = decode_frame(payload, asterix_mod)
        log.info(
            "Frame #%d  key=%s  len=%d bytes  %s",
            frame_count, sample.key_expr, len(payload), description,
        )

    subscriber = session.declare_subscriber(args.zenoh_key, on_sample)
    log.info("Subscribed to '%s'. Press Ctrl+C to stop.", args.zenoh_key)

    running = True

    def _stop(signum, _frame):  # noqa: ANN001
        nonlocal running
        log.info("Signal %d received – stopping subscriber …", signum)
        running = False

    signal.signal(signal.SIGINT, _stop)
    signal.signal(signal.SIGTERM, _stop)

    while running:
        time.sleep(0.5)

    log.info("Total frames received: %d", frame_count)
    subscriber.undeclare()
    session.close()
    log.info("Subscriber stopped.")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Subscribe to raw Asterix frames published on Zenoh.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--zenoh-key", default=DEFAULT_ZENOH_KEY,
        help="Zenoh key expression to subscribe to.",
    )
    parser.add_argument(
        "--zenoh-connect", default=None, metavar="LOCATOR",
        help="Zenoh endpoint locator (e.g. tcp/127.0.0.1:7447).",
    )
    return parser.parse_args(argv)


def main() -> None:
    args = parse_args()
    run_subscriber(args)


if __name__ == "__main__":
    main()
