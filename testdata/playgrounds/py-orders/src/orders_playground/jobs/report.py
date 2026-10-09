from ..lib.logger import logger
from ..services.order_service import order_service


def print_report() -> dict[str, int]:
    report = {"orders": order_service.count}
    logger.info("Orders report", report)
    return report


def report_metrics() -> dict[str, int]:
    metrics = {"pending": sum(order["status"] == "pending" for order in order_service.list())}
    logger.info("Orders metrics", metrics)
    return metrics
