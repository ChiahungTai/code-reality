from pkg.core import Greeter, Plain as P, top_fn


def run(n):
    local_greeter = Greeter()
    handler = top_fn
    extra = P()
    return top_fn(local_greeter) + n
