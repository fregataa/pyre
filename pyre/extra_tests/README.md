# pyre/extra_tests

Pure-Python snippet tests imported from
[RustPython's `extra_tests/snippets`](https://github.com/RustPython/RustPython/tree/main/extra_tests/snippets).

Each `snippets/*.py` is a self-contained script that asserts a small
piece of CPython semantics.  A snippet "passes" when the interpreter
exits with code 0.

## Running

```sh
python3 pyre/extra_tests/run.py            # all three backends
python3 pyre/extra_tests/run.py --cpython-only
python3 pyre/extra_tests/run.py --filter builtin_dict
python3 pyre/extra_tests/run.py -v         # show every (script, backend)
```

The runner sets `cwd` to `snippets/` so `from testutils import ...`
works.  `testutils.py` is the helper module shipped with the snippets
(`assert_raises`, `TestFailingBool`, etc.).

## Layout

- `snippets/` — imported RustPython suite, breadth-first surface
  coverage.  Runner: `pyre/extra_tests/run.py`.
- `parity_tests/` — pyre-authored scripts that pin specific PyPy
  invariants line-by-line.  Each script cites the upstream
  file:line it guards; passing requires `exit 0` AND the final
  stdout line being `OK`.  Runner: `pyre/extra_tests/parity_tests/run.py`.

Both runners share the same backend discovery (cpython +
pyre-dynasm + pyre-cranelift) and exit code semantics.

## Source

Imported from `RustPython/extra_tests/snippets/` (190 `.py` files +
`testutils.py`).  Future updates: pull from upstream when the snippet
surface changes; pyre-specific additions stay in `parity_tests/`.
