from datetime import datetime, timedelta, timezone

from ..lib.logger import logger
from ..services.order_service import order_service


def cleanup_expired_orders() -> int:
    expires_before = datetime.now(timezone.utc) - timedelta(days=1)
    purge_before = expires_before - timedelta(days=6)
    expired: list[str] = []
    for order in order_service.list():
        created_at = datetime.fromisoformat(order["created_at"])
        if order["status"] == "pending" and created_at < expires_before:
            order_service.cancel(order["id"])
        if order["status"] != "paid" and created_at < purge_before:
            expired.append(order["id"])
    removed = 0
    while expired:
        if order_service.remove(expired.pop()):
            removed += 1
    logger.info("Expired orders cleaned up", {"removed": removed})
    return removed
