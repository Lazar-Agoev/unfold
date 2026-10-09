from math import isfinite
from typing import cast

from ..types import OrderDraft


def validate_order(value: object) -> OrderDraft:
    if not isinstance(value, dict):
        raise ValueError("Order must be an object")
    elif not isinstance(value.get("customer_id"), str) or not value["customer_id"]:
        raise ValueError("customer_id must be a nonempty string")
    elif not isinstance(value.get("email"), str) or "@" not in value["email"]:
        raise ValueError("email must contain @")
    items = value.get("items")
    if not isinstance(items, list) or not items:
        raise ValueError("items must be a nonempty array")
    for item in items:
        if not isinstance(item, dict) or not isinstance(item.get("sku"), str):
            raise ValueError("Every item needs a sku")
        quantity, price = item.get("quantity"), item.get("price")
        if type(quantity) is not int or quantity < 1:
            raise ValueError("quantity must be a positive integer")
        if type(price) not in (int, float) or not isfinite(price) or price <= 0:
            raise ValueError("price must be positive and finite")
    return cast(OrderDraft, value)
