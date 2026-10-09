"""Decorator provenance matters; an identical spelling is not enough."""
from functools import cache as cached, lru_cache
from typing import overload

@cached
def transparent(value):
    return value

def unknown(fn):
    return fn

@unknown
def wrapped():
    return 1

transparent(1)
wrapped()

def cache(fn):
    return fn

@cache
def decoy():
    return 1

decoy()

@overload
def overloaded(value: int) -> int: ...
@overload
def overloaded(value: str) -> str: ...
def overloaded(value):
    return value

overloaded(1)

class Accessor:
    @property
    def callback(self):
        return wrapped
    @callback.setter
    def callback(self, value):
        pass
    def run(self):
        return self.callback()
