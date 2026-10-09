from functools import partial

from . import fallback


def format_order(prefix: str, order_id: str, *, suffix: str = "") -> str:
    return f"{prefix}{order_id}{suffix}"


def replacement_label() -> str:
    return "replacement"


def monkey_patch() -> str:
    fallback.public_label = replacement_label
    return fallback.public_label()


def examples(name: str = "upper") -> tuple[str, str, str, str]:
    args = ("order/", "123")
    kwargs = {"suffix": ".json"}
    prefixed = partial(format_order, "order/")
    dispatch = getattr("receipt", name)
    return (
        format_order(*args, **kwargs),
        format_order(order_id="123", prefix="named/", suffix=".json"),
        prefixed("123"),
        dispatch(),
    )
