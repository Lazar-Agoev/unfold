from functools import cache
@cache
def cached(): pass
def changed(): pass
changed = cached
def wrapper(fn): return lambda: 1
@wrapper
def wrapped(): pass
wrapped_alias = wrapped
