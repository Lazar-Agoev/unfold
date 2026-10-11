from dataclasses import dataclass
from typing import Generic, TypeVar

from pydantic import BaseModel

from .models import Address, CartItem

T = TypeVar("T")


@dataclass
class Timestamped:
    created_at: str


@dataclass
class OrderDraft(Timestamped):
    customer_id: str
    items: list[CartItem]


# A dataclass chain; `Generic[T]` is a base from the standard library, `T` a plain link.
@dataclass
class Order(OrderDraft, Generic[T]):
    id: str
    payload: T


class Profile(BaseModel):
    email: str


class Member(Profile):
    tier: str


# A pydantic chain, with `Profile` reached twice; a keyword argument is no base.
class GoldMember(Member, Profile, frozen=True):
    perks: list[str]


class Delivery(Address, total=False):
    eta: str


def make_base() -> type:
    return object


# A base that is a call names no type.
class Dynamic(make_base()):
    note: str
