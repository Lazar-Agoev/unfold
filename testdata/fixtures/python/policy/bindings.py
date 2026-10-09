"""Static policy inputs only; never execute this module."""

def stable():
    return 1

def replacement():
    return 2

stable()
alias = stable
alias()

def reassigned():
    return 1

reassigned = replacement
reassigned()

def globally_written():
    return 1

def write_global():
    global globally_written
    globally_written = replacement

globally_written()

def deleted():
    return 1

del deleted
deleted()

def loop_target():
    return 1

for loop_target in []:
    pass
loop_target()

def with_target():
    return 1

with unknown_manager() as with_target:
    pass
with_target()

def except_target():
    return 1

try:
    pass
except Exception as except_target:
    pass
except_target()

def walrus_target():
    return 1

if (walrus_target := replacement):
    pass
walrus_target()

def outer():
    callback = stable
    def mutate():
        nonlocal callback
        callback = replacement
    callback()
    return mutate

def shadowed_parameter(stable):
    stable()

def comprehension_scope():
    [stable() for stable in []]
    return stable()
