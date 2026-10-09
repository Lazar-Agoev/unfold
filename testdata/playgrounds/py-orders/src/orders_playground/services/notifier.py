from abc import ABC, abstractmethod

from ..lib.logger import logger
from ..types import Order
from .events import events


class Notifier(ABC):
    @abstractmethod
    def send(self, order: Order) -> None:
        raise NotImplementedError

    def notify(self, order: Order) -> None:
        self.send(order)


class ReceiptNotifier(Notifier):
    def send(self, order: Order) -> None:
        logger.info("Order receipt ready", {"order_id": order["id"], "to": order["email"]})


notifier = ReceiptNotifier()


def notify_customer(order: Order) -> None:
    notifier.notify(order)


events.on("order.placed", notify_customer)
