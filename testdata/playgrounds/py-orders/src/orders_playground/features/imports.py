import orders_playground.features.exported
from typing import TYPE_CHECKING

from orders_extensions.labels import namespace_label
from .exported import *

if TYPE_CHECKING:
    from ..types import Order

try:
    from .exported import public_label as selected_label
except ImportError:
    from .fallback import public_label as selected_label


def examples() -> tuple[str, str, str, str]:
    return (
        public_label(),
        selected_label(),
        namespace_label(),
        orders_playground.features.exported.public_label(),
    )
