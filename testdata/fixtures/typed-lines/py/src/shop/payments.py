from decimal import Decimal

from .errors import CardDeclined
from .models import Cart, Customer, Money, Receipt


class Gateway:
    def __init__(self, key: str, *, retries: int = 3) -> None:
        self.key = key
        self.retries = retries

    def charge(
        self,
        cart: Cart,
        customer: Customer,
        /,
        currency: str = "usd",
        *,
        idempotency: str | None = None,
    ) -> Receipt:
        if not cart.items:
            raise CardDeclined(customer.id)
        return Receipt(charge_id="ch_1", amount=self.total(cart))

    def total(self, cart: "Cart") -> Money:
        return sum((item.price * item.quantity for item in cart.items), Decimal(0))

    @staticmethod
    def fee(amount: Decimal) -> Decimal:
        return amount / 100

    @classmethod
    def from_env(cls, *args: str, **options: str) -> "Gateway":
        return cls("sk_env", retries=len(args))
