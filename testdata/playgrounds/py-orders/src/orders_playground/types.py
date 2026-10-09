from typing import TypedDict

ORDER_STATUSES = ("pending", "paid", "cancelled")


class OrderItem(TypedDict):
    sku: str
    quantity: int
    price: float


class OrderDraft(TypedDict):
    customer_id: str
    email: str
    items: list[OrderItem]


class Order(OrderDraft):
    id: str
    status: str
    created_at: str
