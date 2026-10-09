from collections.abc import Callable


class Worker:
    def __init__(self, name: str, handler: Callable[[], None]):
        self.name = name
        self.handler = handler

    def run(self) -> None:
        self.handler()
