import json
import logging

from shop.audit import audit
from shop.format import order_label, summary
from shop.payments import charge_card
from shop.receipts import build_receipt
from shop.store import save_order

logger = logging.getLogger(__name__)


def checkout(order):
    save_order(build_receipt(order, charge_card(order.card)))
    order.status = "paid"
    for line in order.lines:
        line.locked = True
    logger.info("checked out %s", order_label(order))
    audit.record("checkout", json.dumps(summary(order)))
