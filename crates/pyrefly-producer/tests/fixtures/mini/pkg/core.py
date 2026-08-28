CONSTANT = 1


class Greeter:
    tag = "g"

    def __init__(self):
        self.storage = None

    def greet(self):
        return "hi"

    def call_helper(self, fn):
        return fn()


class Plain:
    label = "p"


class Wrapper:
    class Inner:
        pass


def top_fn(x):
    return x + CONSTANT


def make():
    g = Greeter()
    p = Plain()
    inner = Wrapper.Inner()

    def inner_helper():
        return g.greet()

    inner_helper()
    assert isinstance(p, Plain)
    return top_fn(g)
