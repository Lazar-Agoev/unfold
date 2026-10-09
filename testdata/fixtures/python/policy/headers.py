"""Header calls remain source relationships without executing annotations."""


def type_factory():
    return int


def value_factory():
    return 1


def decorator_factory(value):
    return lambda fn: fn


@decorator_factory(value_factory())
def declared(value: type_factory() = value_factory(), *, named=value_factory()) -> type_factory():
    return value_factory()


class Base:
    pass


def base_factory():
    return Base


def metaclass_factory():
    return type


@decorator_factory(value_factory())
class Derived(base_factory(), metaclass=metaclass_factory()):
    field: type_factory()


callback = lambda value=value_factory(): value_factory()
declared()
