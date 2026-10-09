import argparse
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source")
    parser.add_argument("destination", nargs="?")
    args = parser.parse_args()
    records = json.loads(Path(args.source).read_text(encoding="utf-8"))
    result = json.dumps([{"id": row.get("id"), "customer_id": row["customer_id"]} for row in records])
    if args.destination:
        with Path(args.destination).open("x", encoding="utf-8") as output:
            output.write(result)
    else:
        print(result)
