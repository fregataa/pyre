"""Phase 6 parity test: `_check_code_mutable` AttributeError reach.

PyPy `function.py:385-388 _check_code_mutable`:

    def _check_code_mutable(self, attr):
        if not self.can_change_code:
            raise oefmt(self.space.w_AttributeError,
                        "Cannot change %s attribute of builtin functions", attr)

Hit indirectly through every `fset_func_*` and `fdel_func_*` that
mutates a builtin's metadata.  Pinned contract:
  - builtin function (e.g. `print`, `len`) setters raise
    AttributeError, not TypeError or silent-ignore,
  - the AttributeError fires for `__name__`, `__qualname__`, `__doc__`,
    `__defaults__`, `__kwdefaults__`, and `__annotations__`.

The error type alone is the contract — we don't assert the exact
message because CPython / PyPy diverge ("readonly attribute" vs the
"Cannot change ... of builtin functions" phrasing).

`__module__` is intentionally excluded: CPython's
`builtin_function_or_method` keeps `__module__` writable through its
own descriptor, so `print.__module__ = 'x'` succeeds on CPython.  Pyre
follows CPython here for cross-runtime stability.  `__code__` is also
excluded because CPython's builtin doesn't expose it at all (NoSlot).
"""

def _expect_attr_error(action, label):
    try:
        action()
    except AttributeError:
        return
    raise AssertionError(f"{label}: expected AttributeError")


for attr in (
    "__name__",
    "__qualname__",
    "__doc__",
    "__defaults__",
    "__annotations__",
):
    _expect_attr_error(
        lambda a=attr: setattr(print, a, "x"),
        f"setattr builtin .{attr}",
    )

# `__kwdefaults__` is intentionally excluded: PyPy's
# `fset_func_kwdefaults` (`function.py:427-433`) has NO
# `_check_code_mutable` call and accepts `None` even on builtins,
# silently writing through `self.w_kw_defs`.  CPython rejects with
# AttributeError; PyPy/pyre accept.  Pyre follows PyPy here per
# the line-by-line port.

# `del print.__doc__` is rejected — PyPy `fdel_func_doc` goes through
# `_check_code_mutable`; CPython agrees.
_expect_attr_error(lambda: delattr(print, "__doc__"),
                   "delattr builtin __doc__")


# A user function is mutable — no AttributeError on the same setters
# (regression guard against over-broad gating).
def _h(a, b=2):
    "h doc"
    return a + b

_h.__name__ = "renamed"
_h.__qualname__ = "renamed_q"
_h.__doc__ = "new"
_h.__defaults__ = (10,)
del _h.__doc__

print("OK")
