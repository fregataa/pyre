"""Phase 6 parity test: cell object protocol via __closure__.

PyPy `nestedscope.py:22-125 Cell` + `typedef.py:934-952 Cell.typedef`:

    Cell.typedef = TypeDef("cell",
        ...
        cell_contents = GetSetProperty(
            Cell.descr__cell_contents,
            Cell.descr_set_cell_contents,
            Cell.descr_del_cell_contents,
            cls=Cell),
    )

`descr__cell_contents` returns the inner value or raises
`ValueError("Cell is empty")` when unset (`nestedscope.py:112-116`).
`descr_set_cell_contents` writes the value (`:118-119`).
`descr_del_cell_contents` clears to empty and silently swallows the
`ValueError` for a re-delete (`:121-125`).

Pinned contract:
  1. `f.__closure__[i]` returns a `cell` (not the unwrapped value),
  2. `cell.cell_contents` reads the captured value,
  3. assignment to `cell.cell_contents` writes through the cell so
     the inner function observes the new value,
  4. `del cell.cell_contents` clears the cell; subsequent reads raise
     `ValueError`.
"""

def _make(x):
    def _inner():
        return x
    return _inner

i = _make(10)

# (1) cell identity at the closure tuple position.
c = i.__closure__[0]
assert type(c).__name__ == "cell", f"type(c): {type(c).__name__!r}"

# (2) read cell_contents.
assert c.cell_contents == 10, f"initial cell_contents: {c.cell_contents!r}"

# (3) writing cell_contents propagates to the captured closure.
c.cell_contents = 99
assert c.cell_contents == 99
assert i() == 99, f"inner() after rebind: {i()!r}"

# (4) deleting cell_contents clears the cell; re-read → ValueError.
del c.cell_contents
try:
    _ = c.cell_contents
except ValueError:
    pass
else:
    assert False, "reading cleared cell must raise ValueError"

# Repeating the del is silently ignored per nestedscope.py:121-125.
del c.cell_contents
# After a fresh write the cell is alive again.
c.cell_contents = 7
assert c.cell_contents == 7

print("OK")
