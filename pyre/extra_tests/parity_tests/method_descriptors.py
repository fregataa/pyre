"""Phase 6 parity test: method / staticmethod / classmethod descriptors.

Captures the `interp_attrproperty_w` line-by-line port for the
six descriptors landed across the recent typedef commits:

  Method.__func__       /  Method.__self__
  staticmethod.__func__ /  staticmethod.__wrapped__
  classmethod.__func__  /  classmethod.__wrapped__

PyPy bindings: `pypy/interpreter/typedef.py:839-840` (Method),
`:870-871` (StaticMethod), `:884-885` (ClassMethod).  All six route
through the `interp_attrproperty_w` fget shape (typedef.py:465-474):
"fetch the named slot; substitute `space.w_None` when it is None".

This file documents the working contract so a later commit that
breaks identity (`bm.__func__ is m`) or aliasing
(`sm.__func__ is sm.__wrapped__`) trips an AssertionError pointing
at the exact descriptor that regressed.
"""

# ── Method.__func__ / __self__ ─────────────────────────────────────
class _C:
    def m(self):
        return 1

_inst = _C()
_bm = _inst.m

assert type(_bm).__name__ == "method", (
    f"bound method type: got {type(_bm).__name__!r}"
)
assert _bm.__func__ is _C.m, (
    "bound method.__func__ must be the underlying function"
)
assert _bm.__self__ is _inst, (
    "bound method.__self__ must be the binding instance"
)


# ── staticmethod.__func__ / __wrapped__ ────────────────────────────
def _f():
    return 2

_sm = staticmethod(_f)

assert _sm.__func__ is _f, "sm.__func__ must be the wrapped function"
assert _sm.__wrapped__ is _f, "sm.__wrapped__ must be the wrapped function"
assert _sm.__func__ is _sm.__wrapped__, (
    "sm.__func__ and sm.__wrapped__ must be the same object (both alias w_function)"
)


# ── classmethod.__func__ / __wrapped__ ─────────────────────────────
_cm = classmethod(_f)

assert _cm.__func__ is _f, "cm.__func__ must be the wrapped function"
assert _cm.__wrapped__ is _f, "cm.__wrapped__ must be the wrapped function"
assert _cm.__func__ is _cm.__wrapped__, (
    "cm.__func__ and cm.__wrapped__ must be the same object (both alias w_function)"
)


# ── Cross-instance: staticmethod and classmethod each carry their own w_function ──
def _g():
    return 3

_sm_g = staticmethod(_g)
_cm_g = classmethod(_g)
assert _sm.__func__ is not _sm_g.__func__, (
    "distinct staticmethod instances must reference distinct functions"
)
assert _cm.__func__ is not _cm_g.__func__, (
    "distinct classmethod instances must reference distinct functions"
)

print("OK")
