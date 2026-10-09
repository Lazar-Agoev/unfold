"""Builtin evaluation differs from same-spelling local declarations."""


def target():
    return 1


def unaffected():
    return target()


def unbounded_evaluation(text):
    eval(text)
    return target()


def spelling_decoy():
    def eval(value):
        return value

    eval("plain text")
    return target()
