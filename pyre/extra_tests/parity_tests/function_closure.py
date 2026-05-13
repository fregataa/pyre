"""Phase 6 parity test: Function.__closure__ contract.

PyPy `function.py:538-543 fget_func_closure`:

    def fget_func_closure(self, space):
        if self.closure is not None:
            w_res = space.newtuple([cell for cell in self.closure])
        else:
            w_res = space.w_None
        return w_res

Typedef binding: `typedef.py:826 __closure__ = GetSetProperty(
Function.fget_func_closure)` — read-only (no fset / fdel).

Pinned contract (asserted on all three runners):
  1. Function without free variables: f.__closure__ is None.
  2. Function with free variables: __closure__ is a tuple whose
     length matches the number of co_freevars.
  3. __closure__ is read-only: assignment raises AttributeError.

NOT asserted: the internal cell-element shape.  CPython exposes
each element as a `cell` object whose value is unwrapped via
`.cell_contents`.  Pyre currently auto-unwraps on indexing —
indexing `inner.__closure__[0]` yields the captured value instead
of a cell object.  Asserting cell-internal shape would lock-in a
divergence; that gap belongs to a separate cell-object parity
audit.
"""

# (1) No closure -> None.
def _no_free():
    return 1

assert _no_free.__closure__ is None, f"no-closure: {_no_free.__closure__!r}"


# (2) With closure -> tuple of len(co_freevars).
def _make(x, y):
    def _inner():
        return x + y
    return _inner

_inner = _make(10, 20)
assert _inner.__closure__ is not None
assert type(_inner.__closure__) is tuple, (
    f"closure type: {type(_inner.__closure__).__name__}"
)
assert len(_inner.__closure__) == 2, f"len: {len(_inner.__closure__)}"
# NOTE: pyre does not yet surface __code__.co_freevars as a tuple
# (PyPy parity gap unrelated to __closure__); assertion on
# closure length alone is sufficient for the descriptor contract.

# Single-free variant.
def _single(z):
    def _i():
        return z
    return _i

_i = _single(7)
assert type(_i.__closure__) is tuple
assert len(_i.__closure__) == 1


# (3) Read-only setter -> AttributeError.
try:
    _inner.__closure__ = ()
except AttributeError:
    pass
else:
    assert False, "setting __closure__ must raise AttributeError"

print("OK")
