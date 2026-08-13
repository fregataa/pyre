# Ubuntu 24.04 amd64 repro image

This image fixes the Linux userspace used to reproduce Ubuntu-only
`pyre-cranelift` failures. It intentionally does not copy the repository into
the image; mount the worktree at runtime so local edits and build artifacts are
visible.

Build with Apple `container`:

```bash
container build --platform linux/amd64 -m 8G -c 4 --progress plain \
  -t pyre-ubuntu24-amd64-repro \
  tools/ubuntu24-amd64-repro
```

Run from the repository root:

```bash
mkdir -p "$(dirname "$PWD")/.pyre-build"
MAIN=$(sed -n 's|^gitdir: \(.*\)/\.git/worktrees/.*|\1|p' .git)   # empty in a normal clone
container run --rm --platform linux/amd64 --rosetta -m 20G -c 4 \
  --mount type=bind,source="$(pwd)",target=/workspace/pyre \
  --mount type=bind,source="$(dirname "$PWD")/.pyre-build",target=/workspace/.pyre-build \
  --mount type=bind,source="$MAIN",target="$MAIN" \
  pyre-ubuntu24-amd64-repro
```

The second mount keeps the shared Charon cache outside the worktree and reuses
it across sibling worktrees and container runs. The third is needed when the
repository is a `git worktree`: its `.git` is a file naming an absolute path
into the main checkout, and `extract-llbc.py` fingerprints the tree with
`git ls-files`, so without that path — at the *same* location inside — every
git call fails with `fatal: not a git repository`. Both trees also need
`git config --global --add safe.directory <path>` inside the container.

Inside the container. `CARGO_TARGET_DIR` keeps the container's Linux objects out
of the host's `target/`, which the bind mount otherwise shares; and
`extract-llbc.py` takes no crate arguments, because naming a subset mixes fresh
and stale artefacts. Give `CARGO_TARGET_DIR` an absolute path — the extract runs
charon from each crate's own directory, so a relative value drops a separate
multi-gigabyte `target-linux/` into every crate instead of sharing one.

```bash
python3 scripts/install-charon.py
charon toolchain-path                 # one-time, installs the pinned nightly
export CARGO_TARGET_DIR=/workspace/pyre/target-linux   # absolute: see below
CARGO_INCREMENTAL=0 python3 scripts/extract-llbc.py
CARGO_INCREMENTAL=0 cargo build --release -p pyrex --bin pyre-cranelift \
  --no-default-features --features cranelift
RUST_BACKTRACE=1 ./target-linux/release/pyre-cranelift pyre/bench/synth/bool_compare.py
```

`pyre/check.py` cannot run here: it requires CPython 3.14 and `pypy3`, and this
image carries neither. Use the container for crashes and wrong answers, and
`pyre/cpython_tests/run.py` for the CPython-suite gate.
