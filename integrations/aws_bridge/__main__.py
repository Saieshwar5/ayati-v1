"""Run a configured bridge as a separately supervised process in the release image."""
import argparse
import logging
import os
from pathlib import Path
import signal
import threading

from .config import private_json
from .startup import BridgeRuntime, private_path, reject_core_secrets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path, help="Absolute owner-owned 0600 JSON configuration file")
    args = parser.parse_args()
    stop = threading.Event()
    for event in (signal.SIGINT, signal.SIGTERM):
        signal.signal(event, lambda *_: stop.set())
    try:
        reject_core_secrets(os.environ)
        config = private_json(private_path(str(args.config)))
        with BridgeRuntime(config):
            print("Bridge listeners ready (loopback only)", flush=True)
            stop.wait()
    except Exception as error:
        logging.error("Bridge startup failed (%s)", type(error).__name__)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
