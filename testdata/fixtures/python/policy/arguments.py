"""Written argument ordinals do not imply Python parameter bindings."""
def handler(value):
    return value

def invoke(fn, value, *, keyword_handler=handler):
    fn(value)
    keyword_handler(value)

invoke(handler, 1)
invoke(value=1, fn=handler)
invoke(handler, 1, keyword_handler=handler)

def expanded(args, kwargs):
    invoke(*args, **kwargs)

class Receiver:
    def invoke(self, fn, value):
        return fn(value)

def Kelvin():
    return 1

Kelvin()
