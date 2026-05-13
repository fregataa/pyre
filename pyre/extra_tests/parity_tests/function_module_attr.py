"""Phase 6 parity test: Function.__module__ lazy-cache contract.

PyPy `function.py:503-509 fget___module__`:

    def fget___module__(self, space):
        if self.w_module is None:
            if self.w_func_globals is not None and not space.is_w(
                    self.w_func_globals, space.w_None):
                self.w_module = space.call_method(
                    self.w_func_globals, "get", space.newtext("__name__"))
            else:
                self.w_module = space.w_None
        return self.w_module

Wired via typedef.py `getset___module__` on Function.typedef.  Pyre
mirrors the lazy-cache shape but reads from the raw DictStorage
backing the function's globals rather than calling
`space.call_method(..., "get", ...)` (Phase 5 retires this gap).

The user-visible contract holds for both CPython and PyPy:

  1. First read of `f.__module__` looks up `__name__` in the
     defining module's globals at first-access time and caches it.
  2. Subsequent reads return the cached value even if the source
     globals mutates `__name__` afterwards.
  3. A function defined after the mutation sees the new value on
     its own first read.
"""

import sys
_module = sys.modules[__name__]
_module.__name__ = "FIRST"

def _f():
    pass

# First read: lazy resolve from globals["__name__"].
assert _f.__module__ == "FIRST", f"first read: {_f.__module__!r}"

# Mutate the source.
_module.__name__ = "SECOND"

# _f still reports the cached value — its w_module slot was stamped
# on the first access above.
assert _f.__module__ == "FIRST", (
    f"cached __module__ must survive globals mutation: {_f.__module__!r}"
)

# A function created after the mutation picks up the new value.
def _g():
    pass

assert _g.__module__ == "SECOND", f"new function: {_g.__module__!r}"

# Direct setter overrides the cache.
_f.__module__ = "OVERRIDDEN"
assert _f.__module__ == "OVERRIDDEN"

# Deleter resets to None.
del _f.__module__
assert _f.__module__ is None, f"after del: {_f.__module__!r}"

print("OK")
