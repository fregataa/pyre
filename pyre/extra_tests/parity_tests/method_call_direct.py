"""Phase 6 parity test: bound `method` call protocol.

PyPy `function.py:563-583 _Method`:

    def call_args(self, args):
        space = self.space
        return space.call_obj_args(self.w_function, self.w_instance, args)

    def descr_method_call(self, __args__):
        return self.call_args(__args__)

`Method.__call__` calls the wrapped function with `self.w_instance`
prepended as the first argument.  Tested invariants:
  1. `obj.m()` returns the same as `bm = obj.m; bm()` — binding caches
     the receiver.
  2. The wrapper is type `method` (PyPy's `_Method.typedef`).
  3. `bm.__func__` is the underlying function; `bm.__self__` is the
     bound receiver — the inputs `call_args` recombines.
  4. Calling `bm` with additional positional / keyword args threads
     them through to the underlying function after `self`.
"""

class _C:
    def m(self, x, *, k=1):
        return ("m", self.tag, x, k)

c = _C()
c.tag = "alpha"

# (1) Direct call.
assert c.m(7) == ("m", "alpha", 7, 1)

# (2) Cached bound method behaves identically.
bm = c.m
assert type(bm).__name__ == "method"
assert bm(7) == ("m", "alpha", 7, 1)
assert bm(8, k=99) == ("m", "alpha", 8, 99)

# (3) __func__ / __self__ shape.
assert bm.__self__ is c, f"__self__: {bm.__self__!r}"
assert bm.__func__ is _C.__dict__['m']  # the raw function in the class dict.

# (4) Re-bind on a fresh instance — receiver follows.
d = _C()
d.tag = "beta"
bm2 = d.m
assert bm2(0) == ("m", "beta", 0, 1)

# Sanity: original binding is unaffected.
assert bm(0) == ("m", "alpha", 0, 1)

print("OK")
