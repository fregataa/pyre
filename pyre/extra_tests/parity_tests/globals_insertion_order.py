"""Phase 6 baseline: mixed-key insertion order in globals().

Verifies that the globals dict preserves insertion order across:
  - top-level `name = value` assignments,
  - `globals()[k] = v` direct stores,
  - `exec("k = v", globals())` execution into the same dict,
  - `del globals()[k]` removals.

Order preservation has been a hard contract on dict since
Python 3.7 (PEP 468 + dict ordering guarantee).  In PyPy the same
holds for `W_DictObject` per `dictmultiobject.py:30-50`.  In pyre,
both the `DictStorage` insertion-ordered Vec AND the paired
`W_DictObject.entries` must agree on order — Phase 5 work that
collapses the dual storage must keep this invariant.
"""

# Top-level assignments interleaved with direct globals() writes.
alpha = 1
globals()["beta"] = 2
gamma = 3
globals()["delta"] = 4

# Exec-driven insert into our own globals dict.
exec("epsilon = 5", globals())

# Mutate (does not reorder).
beta = 22

# Remove + re-add (re-add must land at the tail).
del globals()["alpha"]
alpha = 11  # noqa: F811 — intentional reinsertion

# Now check the iteration order — alpha was removed then re-added,
# so it must appear LAST, and the remaining keys must keep the
# original interleaved order.
g = globals()
keys = [k for k in g if k in {"alpha", "beta", "gamma", "delta", "epsilon"}]
assert keys == ["beta", "gamma", "delta", "epsilon", "alpha"], (
    f"insertion order broken: got {keys!r}"
)

# Values must reflect the latest writes.
assert g["alpha"] == 11
assert g["beta"] == 22
assert g["gamma"] == 3
assert g["delta"] == 4
assert g["epsilon"] == 5

# Iterating items() must yield (key, value) pairs in the same order.
items = [(k, v) for k, v in g.items() if k in {"alpha", "beta", "gamma", "delta", "epsilon"}]
assert items == [("beta", 22), ("gamma", 3), ("delta", 4), ("epsilon", 5), ("alpha", 11)], (
    f"items() order broken: got {items!r}"
)

# Keys/values views must agree with items().
keys_view = [k for k in g.keys() if k in {"alpha", "beta", "gamma", "delta", "epsilon"}]
assert keys_view == ["beta", "gamma", "delta", "epsilon", "alpha"]

print("OK")
