from pkg.core import Greeter, top_fn


def run(n):
    local_greeter = Greeter()
    return top_fn(local_greeter) + n
