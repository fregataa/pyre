"""Phase 6 parity test: code.co_freevars / co_cellvars contract.

PyPy `pycode.py:335-339`:

    def fget_co_cellvars(self, space):
        return space.newtuple([space.newtext(name)
                               for name in self.co_cellvars])

    def fget_co_freevars(self, space):
        return space.newtuple([space.newtext(name)
                               for name in self.co_freevars])

Pinned contract:
  - tuple of str (possibly empty),
  - co_cellvars enumerates names bound in this code's locals that are
    captured by nested scopes,
  - co_freevars enumerates names this code reads from an enclosing
    scope (a co_cellvars of some outer function),
  - empty function: both are ().
"""

# A function with no free or cell vars.
def _empty():
    pass

assert _empty.__code__.co_freevars == (), f"empty freevars: {_empty.__code__.co_freevars!r}"
assert _empty.__code__.co_cellvars == (), f"empty cellvars: {_empty.__code__.co_cellvars!r}"
assert type(_empty.__code__.co_freevars) is tuple
assert type(_empty.__code__.co_cellvars) is tuple


# Outer function binds `z`; inner reads `z` ⇒ z is in outer's
# co_cellvars and in inner's co_freevars.
def _outer():
    z = 1
    def _inner():
        return z
    return _inner

assert _outer.__code__.co_cellvars == ('z',), (
    f"outer cellvars: {_outer.__code__.co_cellvars!r}"
)
inner = _outer()
assert inner.__code__.co_freevars == ('z',), (
    f"inner freevars: {inner.__code__.co_freevars!r}"
)
assert inner.__code__.co_cellvars == ()


# Multiple closures: order follows declaration.
def _make(x, y):
    def _i():
        return x + y
    return _i

i = _make(10, 20)
assert i.__code__.co_freevars == ('x', 'y'), (
    f"two freevars: {i.__code__.co_freevars!r}"
)

print("OK")
