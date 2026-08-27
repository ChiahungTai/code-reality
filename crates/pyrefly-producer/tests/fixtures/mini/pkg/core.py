CONSTANT = 1


class Greeter:
    tag = "g"

    def __init__(self):
        self.storage = None

    def greet(self):
        return "hi"

    def call_helper(self, fn):
        return fn()


def top_fn(x):
    return x + CONSTANT


def make():
    g = Greeter()

    def inner_helper():
        return g.greet()

    inner_helper()
    return top_fn(g)
