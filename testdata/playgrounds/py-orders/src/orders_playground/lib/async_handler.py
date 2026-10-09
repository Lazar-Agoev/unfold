from collections.abc import Awaitable, Callable
from functools import wraps


def async_handler(fn: Callable[..., Awaitable[object]]) -> Callable[..., Awaitable[object]]:
    @wraps(fn)
    async def wrapped(*args: object, **kwargs: object) -> object:
        return await fn(*args, **kwargs)
    return wrapped
