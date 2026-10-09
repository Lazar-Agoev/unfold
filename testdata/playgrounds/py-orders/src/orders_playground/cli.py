import argparse
import importlib

from .jobs.import_orders import import_orders
from .jobs.retry_failed import retry_failed


def report_command() -> dict[str, int]:
    report = importlib.import_module("orders_playground.jobs.report")
    return report.print_report()


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog="orders")
    parser.add_argument("command", choices=["import", "report", "replay", "retry"])
    parser.add_argument("path", nargs="?", default="orders.json")
    arguments = parser.parse_args(argv)
    match arguments.command:
        case "import":
            import_orders(arguments.path)
        case "report":
            report_command()
        case "replay" | "retry":
            retry_failed()
        case _:
            raise ValueError("Unknown command")


if __name__ == "__main__":
    main()
