"""Phase 6 parity test: PyCode introspection attributes.

PyPy binds these via the W_PyCode TypeDef at
`pypy/interpreter/typedef.py:706-722`:

    co_argcount      = GetSetProperty(PyCode.fget_co_argcount)   (typedef.py:707)
    co_kwonlyargcount= GetSetProperty(PyCode.fget_co_kwonlyargcount)
    co_consts        = GetSetProperty(PyCode.fget_co_consts)
    co_names         = GetSetProperty(PyCode.fget_co_names)
    co_varnames      = GetSetProperty(PyCode.fget_co_varnames)
    co_freevars      = GetSetProperty(PyCode.fget_co_freevars)
    co_cellvars      = GetSetProperty(PyCode.fget_co_cellvars)
    co_filename      = interp_attrproperty_w('w_filename', cls=PyCode) (typedef.py:715)
    co_lnotab        = GetSetProperty(PyCode.fget_co_lnotab)

Pyre routes co_argcount / co_kwonlyargcount / co_name / co_filename /
co_flags / co_varnames through a hardcoded match arm in
`baseobjspace::getattr` (baseobjspace.rs:4618-4633), not through
typedef descriptors.  The visible shape, however, matches CPython /
PyPy exactly; this test pins it so a typedef-routing refactor that
loses the shape is caught at the script level.
"""

def _f(a, b, *, c=1):
    return a + b + c


_code = _f.__code__

# Types match CPython / PyPy.
assert type(_code.co_name).__name__ == "str", (
    f"co_name must be str, got {type(_code.co_name).__name__!r}"
)
assert type(_code.co_filename).__name__ == "str", (
    f"co_filename must be str, got {type(_code.co_filename).__name__!r}"
)
assert type(_code.co_varnames).__name__ == "tuple", (
    f"co_varnames must be tuple, got {type(_code.co_varnames).__name__!r}"
)
assert type(_code.co_argcount).__name__ == "int", (
    f"co_argcount must be int, got {type(_code.co_argcount).__name__!r}"
)

# Values match the declaration.
assert _code.co_name == "_f", f"co_name: {_code.co_name!r}"
assert _code.co_argcount == 2, f"co_argcount: {_code.co_argcount!r}"
assert _code.co_kwonlyargcount == 1, f"co_kwonlyargcount: {_code.co_kwonlyargcount!r}"
assert _code.co_varnames == ("a", "b", "c"), f"co_varnames: {_code.co_varnames!r}"

# Identity: same function object reports the same code object.
assert _f.__code__ is _f.__code__, (
    "f.__code__ must return the same code object on repeat access"
)
# And the code object's co_varnames is a fresh tuple per access — CPython
# allocates a new tuple each call because varnames is materialised from
# the underlying list.  Don't assert identity here; assert equality.
assert _code.co_varnames == _f.__code__.co_varnames

# Empty / parameterless function.
def _g():
    pass

assert _g.__code__.co_name == "_g"
assert _g.__code__.co_argcount == 0
assert _g.__code__.co_kwonlyargcount == 0
assert _g.__code__.co_varnames == ()

print("OK")
