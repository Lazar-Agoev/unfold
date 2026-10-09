import json
from pathlib import Path

from ..lib.logger import logger
from ..lib.validate import validate_order
from ..services.order_service import order_service


def import_orders(path: str = "orders.json") -> dict[str, int]:
    with Path(path).open(encoding="utf-8") as source:
        values = json.load(source)
    if not isinstance(values, list):
        raise ValueError("Orders file must contain an array")
    failures: list[Exception] = []
    imported = 0
    for index, value in enumerate(values):
        try:
            draft = validate_order(value)
            order_service.place(draft)
        except ValueError as error:
            logger.error(f"Could not import row {index + 1}", error)
            failures.append(error)
        else:
            imported += 1
    summary = {"imported": imported, "rejected": len(failures)}
    logger.info("Orders import completed", summary)
    if failures:
        raise ExceptionGroup("Some orders could not be imported", failures)
    return summary
