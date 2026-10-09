from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ..types import Order


class OrderRepository:
    def __init__(self):
        self.orders: dict[str, "Order"] = {}

    def insert(self, order: "Order") -> "Order":
        if order["id"] in self.orders:
            raise ValueError("Order already exists")
        self.orders[order["id"]] = order
        return order

    def find(self, order_id: str) -> "Order | None":
        return self.orders.get(order_id)

    def list(self) -> list["Order"]:
        return list(self.orders.values())

    def remove(self, order_id: str) -> bool:
        return self.orders.pop(order_id, None) is not None

    def cancel(self, order_id: str) -> "Order | None":
        order = self.find(order_id)
        if order:
            order["status"] = "cancelled"
        return order
