from dataclasses import dataclass
from decimal import Decimal
from typing import Annotated, Literal, NamedTuple, Optional, TypedDict

from pydantic import BaseModel

Money = Decimal


@dataclass
class CartItem:
    sku: str
    quantity: int
    price: Money


@dataclass
class Cart:
    id: str
    items: list[CartItem]
    coupon: Optional[str] = None


class Address(TypedDict):
    street: str
    city: str


class Customer(BaseModel):
    id: str
    email: str
    address: "Address"
    tier: Literal["gold", "silver"] = "silver"


class Receipt(NamedTuple):
    charge_id: str
    amount: Annotated[Money, "rounded to cents"]
