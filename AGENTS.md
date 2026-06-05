# AGENTS.md

## How pyre's JIT is built: meta-tracing by source translation

pyre is structured like PyPy. `pyre-interpreter` is the Rust interpreter (the
analog of PyPy's RPython interpreter). **The JIT is not hand-written** —
`majit-translate` reads the interpreter's Rust source and *generates* it:
`front/ast.rs` (parse) → `flowspace/` (flow-graph build, the
`flowcontext.py`/`framestate.py` analog) → `annotator/` (`annrpython.py` type
inference) → `rtyper/` (low-level lowering) → `jit_codewriter/`
(`jtransform.py`/`codewriter.py`, emits JitCode). This is the same pipeline
RPython's translator + `jtransform` run over PyPy's interpreter.

**Consequence — "Rust can't be meta-traced" is never a valid excuse for a
deviation.** Generating the JIT from the interpreter source *is* meta-tracing,
by the same principle: whatever semantics the interpreter source expresses is
what the generated JIT must preserve. A JIT that diverges from the
interpreter's behavior has a *generation defect to fix*, not an inherent
limitation of "the JIT is Rust, not Python." Do not justify a mismatch by
appeal to the implementation language.

### Frame identity must be preserved per frame

PyPy keeps one frame object per inlined Python call — `MIFrame` while tracing,
`BlackholeInterpreter` on resume — each carrying its own
`jitcode → pycode → w_globals → locals`. `LOAD_GLOBAL` reads
`self.get_w_globals()` off the *live* frame (`pyframe.py:128-132`:
`jit.promote(self.pycode).w_globals`); guard-failure resume rebuilds one frame
per encoded jitcode header (`resume.py:1042-1057`). Caller/callee namespace
confusion is therefore *impossible* — there is no shared frame slot.

The frame is the interpreter loop's single **red** input; `pycode` is the
**green**. The generated per-code jitcode must thread that red frame for
**every** frame, including inlined non-portal callees. Collapsing inlined
callees onto a single shared anchor (one `portal_frame_reg`, or a single
bridge-resume root frame) drops the callee's own pycode/globals/locals and
makes a cross-module `LOAD_GLOBAL` resolve against the *caller's* globals.
This whole class of bug (the pycode-`names` miscompile, the LOAD_GLOBAL
namespace mismatch, bridge-resume inline-frame globals, vable-resident root
locals) is one root cause — a *frame-identity collapse*. Fix it by restoring
the per-frame red frame (converging on RPython's 1-red-arg frame shape), never
by baking a single anchor's value as a constant.

## Data structure parity with RPython/PyPy

**Do not casually introduce `HashMap` (or any Rust-native collection) when porting RPython/PyPy code.**

majit and pyre are line-by-line ports. The data structure choice is part of
the port — it must match what RPython/PyPy actually uses, even when a Rust
collection looks more convenient.

### Rules

1. **Look up the RPython/PyPy source first.** Before adding `HashMap`, `HashSet`,
   `BTreeMap`, etc., find the corresponding RPython attribute and check what
   container it uses (`dict`, `list`, an attribute on a class instance, a
   field on `_forwarded`, …). Port that exact shape.

2. **Side-tables are usually wrong.** RPython optimizers store information
   *on the box itself* via `box._forwarded` / `PtrInfo` / `IntBound` /
   descr attributes. If you find yourself reaching for
   `HashMap<OpRef, Something>` to track a per-box property, that is almost
   always a sign you skipped the proper PtrInfo / forwarded slot and are
   inventing a parallel store that RPython does not have. Stop and route
   the data through the existing forwarded/PtrInfo machinery instead.

3. **Borrow-checker workarounds must be minimal and documented.** A
   `HashMap` introduced purely because the borrow checker rejected a more
   direct port is acceptable only when (a) every alternative has been
   tried, (b) the deviation is the smallest possible, and (c) a comment
   cites the RPython original it stands in for. See the
   "majit ↔ RPython Parity Rules" section in `~/.claude/CLAUDE.md`.

4. **Removing an RPython method to "simplify" things is not allowed.**
   If `optimizer.py` defines `ensure_ptr_info_arg0`, the Rust port has
   `ensure_ptr_info_arg0`. Do not delete it because callers can be
   rewritten to a shortcut — the shortcut diverges from RPython and the
   next porter will have no idea why their `heap.py` line-by-line port
   no longer compiles.

### Why

We have already been bitten by this. A previous change deleted
`ensure_ptr_info_arg0` and replaced `arrayinfo.lenbound.make_gt_const(...)`
with a side-table `OptHeap.array_min_lengths: HashMap<OpRef, i64>`. The
side-table then could not be read by `postprocess_arraylen_gc`, so that
function was crippled to a hardcoded `IntBound::nonnegative()`, which then
forced `ExportedValueInfo` to grow a parallel `int_lower_bound` field.
One non-orthodox `HashMap` cascaded into four files of divergence from
RPython. Don't start the cascade.

### When in doubt

Grep RPython:

```
rg -t py 'lenbound|getlenbound|_x86_arglocs|_ll_loop_code' rpython/jit/
```


### Workflow guideline

If RPython stores it on an object attribute, store it on the equivalent
Rust struct field. If RPython stores it on `box._forwarded`, route it
through `OptContext::with_intbound_mut` / `set_ptr_info` / etc. Reach
for `HashMap` only after you have proven that RPython itself uses a
dict-like container in that exact spot.

Add as a top-level section in CLAUDE.md, above any general coding guidelines.\n\n## RPython Parity Rules
- When porting from RPython/PyPy, do STRICT line-by-line structural parity. Do NOT take shortcuts, reimplement from scratch, or declare phases 'complete' without the literal refactor.
- If a parity fix causes regressions, investigate root cause before reverting. Do not declare success if structural alignment was skipped, even if benchmarks pass.
- Always verify which worktree/repo you're in (`git rev-parse --show-toplevel`) before editing. Common worktrees: pypy/main, pypy-pyre, pypy-stdlib, pypy-side.
Add under a ## Git Workflow or ## Testing section.\n\n## Before Committing
- Always run `cargo check` and `cargo test` with `--features dynasm`.
- Run full benchmark suite (all 8 benchmarks) after JIT changes - do not commit if any regress.
- Check `git status` and `git rev-parse --show-toplevel` before staging to confirm correct worktree.
- When rebasing/cherry-picking, verify the fix isn't already on main first (`git log main --grep=...`).
Add under a ## Debugging section.\n\n## Debugging Discipline
- When adding trace/debug logs, verify the code path is actually reached (check gating, feature flags) before running the test.
- For root-cause bugs, do NOT implement workarounds (e.g., builtin fallback modules) - fix the actual interpreter/JIT issue.
