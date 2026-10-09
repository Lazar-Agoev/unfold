from ..services.order_service import order_service
from ..types import Order


def process_order(order: object) -> Order:
    return order_service.place(order)
