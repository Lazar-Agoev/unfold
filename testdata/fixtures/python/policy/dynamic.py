"""Dynamic features retain their structure without invented callable bodies."""
from functools import partial
import importlib
import bindings as module

def target(value):
    return value

prefixed = partial(target, 1)
prefixed()

def dispatch(value, name):
    callback = getattr(value, name)
    callback()

module.stable = target
module.stable()

def patch_unknown(name):
    setattr(module, name, target)

def dynamic_scope(text):
    exec(text)
    target(1)

loaded = importlib.import_module("bindings")
loaded.replacement()

def getattr(value, name):
    return target

getattr(None, "anything")
