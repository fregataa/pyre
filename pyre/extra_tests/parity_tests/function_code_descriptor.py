"""Phase 6 parity test: Function.__code__ getter/setter contract.

PyPy `function.py:519-535`:

    def fget_func_code(self, space):
        return self.getcode()

    def fset_func_code(self, space, w_code):
        from pypy.interpreter.pycode import PyCode
        if not self.can_change_code:
            raise oefmt(space.w_AttributeError,
                        "Cannot change code attribute of builtin functions")
        code = space.interp_w(Code, w_code)
        closure_len = 0
        if self.closure:
            closure_len = len(self.closure)
        if isinstance(code, PyCode) and closure_len != len(code.co_freevars):
            raise oefmt(space.w_ValueError,
                        "%N() requires a code object with %d free vars, not "
                        "%d", self, closure_len, len(code.co_freevars))
        self.code = code

Pinned contract:
  1. f.__code__ is the same code object on repeat reads.
  2. Setting a compatible code object on a user function succeeds and
     the function's runtime behaviour reflects the new code.
  3. Setting __code__ on a builtin (function.py:704
     FunctionWithFixedCode -> can_change_code=False) raises
     AttributeError, not TypeError or silent ignore.
  4. Setting a code object whose co_freevars length disagrees with
     the function's closure length raises ValueError.
"""

# ── (1) Read identity ─────────────────────────────────────────────
def _f(x, y=1):
    return x + y

c0 = _f.__code__
assert type(c0).__name__ == "code", f"type: {type(c0).__name__!r}"
assert c0 is _f.__code__, "f.__code__ must return same code object on repeat reads"


# ── (2) Setter on user function, no closure ───────────────────────
def _g(x):
    return x * 2

_f.__code__ = _g.__code__
assert _f(5) == 10, f"after rebind: f(5) = {_f(5)!r}"


# ── (3) Setter on builtin raises AttributeError ───────────────────
try:
    print.__code__ = _g.__code__
except AttributeError:
    pass
else:
    assert False, "setting __code__ on a builtin must raise AttributeError"


# ── (4) Closure / freevars mismatch ──────────────────────────────
def _make_closure(x):
    def _inner():
        return x
    return _inner

_inner = _make_closure(10)
# _inner has closure of length 1.

def _no_free():
    return 99
# _no_free's code has 0 freevars.

try:
    _inner.__code__ = _no_free.__code__
except ValueError:
    pass
else:
    assert False, "closure / freevars mismatch must raise ValueError"

print("OK")
