from ..lib.logger import logger
from ..types import OrderDraft


class PaymentProvider:
    def charge(self, order: OrderDraft) -> None:
        total = sum(item["price"] * item["quantity"] for item in order["items"])
        if total <= 0:
            raise ValueError("Invalid payment total")
        logger.info("Payment preview", {"customer_id": order["customer_id"], "total": total})


class StripeProvider(PaymentProvider):
    def charge(self, order: OrderDraft) -> None:
        super().charge(order)
        logger.info("Stripe payment recorded in preview mode", order["customer_id"])


def create_provider() -> PaymentProvider:
    return StripeProvider()
