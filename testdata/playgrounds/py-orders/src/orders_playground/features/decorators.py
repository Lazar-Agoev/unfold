from collections.abc import Callable
from functools import cache, lru_cache, wraps
from typing import final, override, overload


def traced(label: str) -> Callable:
    def decorate(fn: Callable) -> Callable:
        @wraps(fn)
        def wrapped(*args: object, **kwargs: object) -> object:
            return fn(*args, **kwargs)
        return wrapped
    return decorate


@traced("invoice")
@lru_cache(maxsize=16)
def invoice_label(order_id: str) -> str:
    return f"Invoice {order_id}"


@cache
def normalize_sku(value: str) -> str:
    return value.strip().upper()


@overload
def stringify(value: int) -> str: ...


@overload
def stringify(value: str) -> str: ...


def stringify(value: int | str) -> str:
    return str(value)


class Named:
    def name(self) -> str:
        return "base"


@final
class Labels(Named):
    def __init__(self, value: str):
        self._value = value

    @property
    def value(self) -> str:
        return self._value

    @value.setter
    def value(self, replacement: str) -> None:
        self._value = replacement

    @staticmethod
    def clean(value: str) -> str:
        return normalize_sku(value)

    @classmethod
    def from_text(cls, text: str) -> "Labels":
        return cls(cls.clean(text))

    @override
    def name(self) -> str:
        return self.clean(self.value)


def examples() -> tuple[str, str]:
    labels = Labels.from_text(" book ")
    labels.value = "manual"
    return invoice_label(labels.name()), stringify(1)
