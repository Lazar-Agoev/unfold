from typing import TypeVar

from .errors import CardDeclined
from .models import Cart, Customer, Receipt
from .payments import Gateway

T = TypeVar("T")


def first(items: list[T]) -> T:
    return items[0]


def create_order(cart: Cart, customer: Customer) -> Receipt:
    gateway = Gateway("sk_test", retries=2)
    try:
        receipt = gateway.charge(cart, customer, currency="eur")
    except CardDeclined:
        raise
    Gateway.charge(gateway, cart, customer)
    log(*cart.items, **{"level": "info"})
    log("created", order=receipt.charge_id)
    return receipt


def log(*messages: object, **fields: str) -> None:
    print(*messages, fields)


def label[K](key: K, value: object) -> str:
    return f"{key}={value}"
