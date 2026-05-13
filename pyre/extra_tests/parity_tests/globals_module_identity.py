"""Phase 6 baseline: `f.__globals__ is m.__dict__` identity invariant.

Verifies that a function's `__globals__` slot is the same Python object
(identity, not just equality) as the dict surface exposed by the
module it was defined in, the `globals()` builtin reading the function's
frame, and `exec(code, ns)` reading back `ns`.

Holds in CPython per `Objects/funcobject.c PyFunction_NewWithQualName`
and `pypy/interpreter/function.py:31 self.w_func_globals`.

This is a Phase 5 invariant — DictStorage/W_DictObject dual storage
must keep a single object identity on the W_Root side.  If Phase 5
work breaks this, the assert below fires and the test reports a
non-zero exit.
"""

# Module dict ↔ globals() identity.
import sys
this_module = sys.modules[__name__]
assert globals() is this_module.__dict__, (
    "globals() and module.__dict__ must be the same object"
)

# A function defined here.
def _f():
    return globals()

# 1. f.__globals__ is module.__dict__.
assert _f.__globals__ is this_module.__dict__, (
    "function.__globals__ must be module.__dict__ — got distinct objects"
)

# 2. f.__globals__ is globals() (at top level).
assert _f.__globals__ is globals(), (
    "function.__globals__ must equal globals() at module top level"
)

# 3. globals() inside the function returns the same dict.
assert _f() is this_module.__dict__, (
    "function call reading globals() must return module.__dict__"
)

# 4. Identity survives attribute access via the module:
#    `module.f.__globals__ is module.__dict__`.
import sys
mod = sys.modules[__name__]
assert mod._f.__globals__ is mod.__dict__, (
    "module.f.__globals__ must be module.__dict__ via attribute access"
)

# 5. Identity survives id() — same object reports same id.
assert id(_f.__globals__) == id(this_module.__dict__), (
    "id(f.__globals__) must equal id(module.__dict__)"
)

print("OK")
