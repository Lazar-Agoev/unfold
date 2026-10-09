from collections.abc import Callable


def schedule(label: str, handler: Callable[[], object]) -> object:
    return handler()
