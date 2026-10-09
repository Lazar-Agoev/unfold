from collections.abc import Callable

from ..types import Order

subscriptions: dict[str, list[Callable[[Order], None]]] = {}


class Events:
    def on(self, name: str, handler: Callable[[Order], None]) -> None:
        subscriptions.setdefault(name, []).append(handler)

    def emit(self, name: str, payload: Order) -> None:
        for handler in subscriptions.get(name, []):
            handler(payload)


events = Events()
