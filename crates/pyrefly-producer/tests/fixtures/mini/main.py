from pkg.core import Greeter, top_fn


def run(n):
    local_greeter = Greeter()
    handler = top_fn
    return top_fn(local_greeter) + n
