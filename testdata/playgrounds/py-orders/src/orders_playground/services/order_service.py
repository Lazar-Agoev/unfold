from datetime import datetime, timezone
from uuid import uuid4

from ..db import OrderRepository, open_database
from ..lib.validate import validate_order
from ..types import ORDER_STATUSES, Order
from . import notifier, payment as payments
from .events import events
from .payment import PaymentProvider


class OrderService:
    repo: OrderRepository
    payments: PaymentProvider

    def __init__(self, repo: OrderRepository, payments: PaymentProvider):
        self.repo = repo
        self.payments = payments

    def place(self, value: object) -> Order:
        draft = validate_order(value)
        self.payments.charge(draft)
        order: Order = {
            **draft, "id": str(uuid4()), "status": ORDER_STATUSES[0],
            "created_at": datetime.now(timezone.utc).isoformat(),
        }
        self.repo.insert(order)
        events.emit("order.placed", order)
        return order

    def list(self) -> list[Order]:
        return self.repo.list()

    def get(self, order_id: str) -> Order | None:
        return self.repo.find(order_id)

    def cancel(self, order_id: str) -> Order | None:
        return self.repo.cancel(order_id)

    def remove(self, order_id: str) -> bool:
        return self.repo.remove(order_id)

    @property
    def count(self) -> int:
        return len(self.repo.list())


order_service = OrderService(open_database(), payments.create_provider())
