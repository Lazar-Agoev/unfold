from collections.abc import Callable

sink: Callable[..., None] = print


def set_sink(replacement: Callable[..., None]) -> None:
    global sink
    sink = replacement


class Logger:
    def info(self, message: str, detail: object = None) -> None:
        sink(message, detail)

    def error(self, message: str, detail: object = None) -> None:
        sink(message, detail)


logger = Logger()
