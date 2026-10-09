from .repository import OrderRepository


def connect() -> OrderRepository:
    return OrderRepository()
