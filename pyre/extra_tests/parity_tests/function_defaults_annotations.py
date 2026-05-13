"""Phase 6 parity test: __defaults__ / __kwdefaults__ / __annotations__ contracts.

Cross-runtime contract pinned here:
  - shape (tuple / dict / NoneType),
  - clear via assignment to None,
  - TypeError on a non-matching type (without asserting the exact
    message text — CPython and PyPy disagree on the wording, and
    pyre follows PyPy per the `interpreter/function.py:414-415,
    432, 555` literals).

PyPy bodies:
  function.py:390-397   fget_func_defaults
  function.py:408-416   fset_func_defaults  ("__defaults__ must be set
                                              to a tuple object or None")
  function.py:418-420   fdel_func_defaults
  function.py:422-425   fget_func_kwdefaults
  function.py:427-433   fset_func_kwdefaults  ("__kwdefaults__ must be a dict")
  function.py:435-436   fdel_func_kwdefaults
  function.py:545-548   fget_func_annotations  (lazy create empty dict)
  function.py:550-556   fset_func_annotations  ("__annotations__ must be a dict")
  function.py:558-560   fdel_func_annotations
"""

# ── __defaults__ ──────────────────────────────────────────────────
def _f(a, b=2, c=3):
    return a + b + c

assert _f.__defaults__ == (2, 3), f"initial: {_f.__defaults__!r}"
assert type(_f.__defaults__) is tuple

_f.__defaults__ = (20, 30)
assert _f.__defaults__ == (20, 30), f"after set tuple: {_f.__defaults__!r}"

_f.__defaults__ = None
assert _f.__defaults__ is None, f"after set None: {_f.__defaults__!r}"

try:
    _f.__defaults__ = [1, 2]
except TypeError:
    pass
else:
    assert False, "setting list on __defaults__ must raise TypeError"


# ── __kwdefaults__ ────────────────────────────────────────────────
def _g(*, x=1, y=2):
    return x + y

assert _g.__kwdefaults__ == {"x": 1, "y": 2}, f"initial: {_g.__kwdefaults__!r}"
assert type(_g.__kwdefaults__) is dict

_g.__kwdefaults__ = {"x": 10}
assert _g.__kwdefaults__ == {"x": 10}, f"after set: {_g.__kwdefaults__!r}"

_g.__kwdefaults__ = None
assert _g.__kwdefaults__ is None, f"after None: {_g.__kwdefaults__!r}"

try:
    _g.__kwdefaults__ = "string"
except TypeError:
    pass
else:
    assert False, "setting string on __kwdefaults__ must raise TypeError"


# ── __annotations__ ───────────────────────────────────────────────
# fget_func_annotations lazy-creates an empty dict the first time it
# is read on a function defined without explicit annotations stamped
# into w_ann (pyre never stamps; PyPy stamps if MAKE_FUNCTION's
# ANNOTATIONS flag fires).  Either way the *contract* is: returns
# a dict, and successive reads return the same dict.
def _h():
    return None

ann1 = _h.__annotations__
assert type(ann1) is dict, f"type: {type(ann1).__name__}"
ann2 = _h.__annotations__
assert ann1 is ann2, "lazy-created annotations dict must keep identity"

# Mutation through one reference is visible through the other.
ann1["sample"] = "value"
assert _h.__annotations__["sample"] == "value"

# Replace via setter — must be a dict.
_h.__annotations__ = {"new": "dict"}
assert _h.__annotations__ == {"new": "dict"}

try:
    _h.__annotations__ = [1, 2]
except TypeError:
    pass
else:
    assert False, "setting list on __annotations__ must raise TypeError"

# Setting None clears the slot (interpreted as `w_ann = None` upstream).
_h.__annotations__ = None
ann3 = _h.__annotations__
assert type(ann3) is dict, f"after clear: {type(ann3).__name__}"
# After clear, next read lazy-creates a fresh dict (identity differs).
assert ann3 is not ann1, "lazy-recreated dict after None reset"

print("OK")
