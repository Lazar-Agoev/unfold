"""Explicit runtime smoke. This program is never invoked while indexing."""

import json
import os
import socket
import subprocess
import sys
import time
from pathlib import Path
from urllib.error import URLError
from urllib.request import Request, urlopen


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    environment = {**os.environ, "PORT": str(port), "BROKER_URL": "memory://", "ORDERS_EAGER": "1"}
    server = subprocess.Popen([sys.executable, "-m", "orders_playground.server"], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.monotonic() + 15
        while True:
            try:
                with urlopen(f"http://127.0.0.1:{port}/health", timeout=1) as response:
                    assert json.load(response) == {"status": "ok"}
                break
            except (URLError, TimeoutError):
                if server.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("API did not start")
                time.sleep(0.05)
        body = (root / "examples/orders.json").read_bytes()
        draft = json.loads(body)[0]
        request = Request(f"http://127.0.0.1:{port}/orders", data=json.dumps(draft).encode(), headers={"Content-Type": "application/json", "x-user": "smoke"}, method="POST")
        with urlopen(request, timeout=3) as response:
            assert response.status == 201
            assert json.load(response)["customer_id"] == "demo"
        subprocess.run([sys.executable, "-m", "orders_playground.cli", "report"], env=environment, check=True, timeout=15)
        subprocess.run([sys.executable, "-m", "orders_playground.cli", "import", str(root / "examples/orders.json")], env=environment, check=True, timeout=15)
        print("PASS: real HTTP API and CLI. Run pytest for eager Celery and one-shot APScheduler coverage.")
    finally:
        server.terminate()
        try:
            _, errors = server.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
            _, errors = server.communicate(timeout=5)
        if server.returncode not in (0, -15):
            print(errors, file=sys.stderr)


if __name__ == "__main__":
    main()
