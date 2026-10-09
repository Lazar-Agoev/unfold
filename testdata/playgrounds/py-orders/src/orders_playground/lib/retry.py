from collections.abc import Callable

from .logger import logger


def with_retry[T](fn: Callable[[], T], attempts: int) -> T:
    if attempts < 1:
        raise ValueError("attempts must be positive")
    attempt = 0
    while attempt < attempts:
        try:
            return fn()
        except Exception as error:
            if attempt + 1 == attempts:
                raise
            logger.error("Operation failed; retrying", error)
        finally:
            attempt += 1
    raise RuntimeError("Retry attempts exhausted")
