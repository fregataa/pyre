"""Phase 6 baseline: single-view invariant across globals surfaces.

The same logical namespace is exposed through several surfaces:
  - `globals()` builtin (reads the running frame's f_globals),
  - `module.__dict__` attribute (reads the module's w_dict),
  - `function.__globals__` attribute (reads the function's w_func_globals),
  - the `ns` argument passed to `exec(code, ns)`.

A write through ANY surface must be visible through every other surface
immediately, because they all reference the same underlying dict.
Conversely, any of them iterating must observe writes made through any
other.

This is the deepest invariant Phase 5 must preserve: the dual-storage
model (DictStorage + lazy W_DictObject sibling via mirror_target) was
introduced precisely to fake this single-view illusion across the
two backing stores.  When Phase 5 collapses to a unified
`PyObjectRef` field, the invariant becomes natural — but during the
transitional commits any drift breaks user-visible identity.
"""

import sys
this_module = sys.modules[__name__]

# Write through globals() — read through module.__dict__ and function.__globals__.
def _f():
    return None

globals()["w1"] = "via_globals"
assert this_module.__dict__["w1"] == "via_globals", (
    "globals() write not visible through module.__dict__"
)
assert _f.__globals__["w1"] == "via_globals", (
    "globals() write not visible through function.__globals__"
)

# Write through module.__dict__ — read through globals() and function.__globals__.
this_module.__dict__["w2"] = "via_moddict"
assert globals()["w2"] == "via_moddict", (
    "module.__dict__ write not visible through globals()"
)
assert _f.__globals__["w2"] == "via_moddict", (
    "module.__dict__ write not visible through function.__globals__"
)

# Write through function.__globals__ — read through globals() and module.__dict__.
_f.__globals__["w3"] = "via_func"
assert globals()["w3"] == "via_func", (
    "function.__globals__ write not visible through globals()"
)
assert this_module.__dict__["w3"] == "via_func", (
    "function.__globals__ write not visible through module.__dict__"
)

# exec() into our own globals: write must be visible to all surfaces.
exec("w4 = 'via_exec'", globals())
assert this_module.__dict__["w4"] == "via_exec", (
    "exec() write not visible through module.__dict__"
)
assert _f.__globals__["w4"] == "via_exec", (
    "exec() write not visible through function.__globals__"
)

# del through one surface — invisible through all.
del globals()["w1"]
assert "w1" not in this_module.__dict__
assert "w1" not in _f.__globals__

del this_module.__dict__["w2"]
assert "w2" not in globals()
assert "w2" not in _f.__globals__

# Capture inside a function so the temporaries land in the function
# frame, not globals (which would change the surface mid-snapshot).
def _snapshot():
    return (
        len(globals()),
        len(this_module.__dict__),
        len(_f.__globals__),
        sorted(globals().keys()),
        sorted(this_module.__dict__.keys()),
        sorted(_f.__globals__.keys()),
    )

len_g, len_m, len_f, keys_g, keys_m, keys_f = _snapshot()
assert len_g == len_m == len_f, (
    f"len() disagrees across globals surfaces: g={len_g} m={len_m} f={len_f}"
)
assert keys_g == keys_m == keys_f, (
    f"sorted keys disagree across surfaces: g={keys_g!r}, m={keys_m!r}, f={keys_f!r}"
)

print("OK")
