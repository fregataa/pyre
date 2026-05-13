"""Phase 6 parity test: Function.__doc__ lazy-init contract.

PyPy `function.py:446-457`:

    def fget_func_doc(self, space):
        if self.w_doc is None:
            self.w_doc = self.code.getdocstring(space)
        return self.w_doc

    def fset_func_doc(self, space, w_doc):
        self._check_code_mutable("__doc__")
        self.w_doc = w_doc

    def fdel_func_doc(self, space):
        self._check_code_mutable("__doc__")
        self.w_doc = space.w_None

The lazy read fills `w_doc` from `code.getdocstring(space)` on first
access; the setter accepts any value (no type validation); the deleter
stamps `space.w_None` (cleared, not "unset").
"""

# Function with docstring.
def _f():
    "this is f"
    return 1

# Lazy first-read: derives from co_consts[0] when set as docstring.
assert _f.__doc__ == "this is f", f"docstring: {_f.__doc__!r}"

# Function without docstring.
def _g():
    return 2

assert _g.__doc__ is None, f"no-doc: {_g.__doc__!r}"

# Setter accepts a string.
_f.__doc__ = "new doc"
assert _f.__doc__ == "new doc"

# Setter accepts any object — PyPy fset_func_doc validates nothing.
_f.__doc__ = 42
assert _f.__doc__ == 42

_f.__doc__ = ["list"]
assert _f.__doc__ == ["list"]

# Setter accepts None.
_f.__doc__ = None
assert _f.__doc__ is None

# Deleter resets to space.w_None — observable as None.
def _h():
    "h doc"
    return 0

assert _h.__doc__ == "h doc"
del _h.__doc__
assert _h.__doc__ is None, f"after del: {_h.__doc__!r}"

# After del, setter still works.
_h.__doc__ = "rebound"
assert _h.__doc__ == "rebound"

print("OK")
