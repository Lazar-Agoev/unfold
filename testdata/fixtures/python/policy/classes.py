"""Construction and MRO policy inputs; parse as data, never execute."""
from typing import final


@final
class Plain:
    def __init__(self):
        pass


def unknown_decorator(value):
    return value


@unknown_decorator
class Wrapped:
    def __init__(self):
        pass


class Meta(type):
    def __call__(cls):
        return Plain()


class Custom(metaclass=Meta):
    def __init__(self):
        pass


class UnknownMeta(metaclass=unavailable_metaclass):
    pass


Plain()
Wrapped()
Custom()
UnknownMeta()


class Root:
    def method(self):
        return 1


class Left(Root):
    pass


class Right(Root):
    pass


class Valid(Left, Right):
    def dispatch(self):
        return self.method()


class UnknownBase(unavailable_base):
    def dispatch(self):
        return self.method()


class LeftFirst(Left, Right):
    pass


class RightFirst(Right, Left):
    pass


class Inconsistent(LeftFirst, RightFirst):
    def dispatch(self):
        return self.method()


class DeletedReceiver:
    def __init__(self):
        self.repo = Root()

    def clear(self):
        del self.repo

    def run(self):
        return self.repo.method()


class PatchedReceiver:
    repo: Root

    def patch(self, name, value):
        setattr(self, name, value)

    def run(self):
        return self.repo.method()
