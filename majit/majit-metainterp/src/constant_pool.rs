//! Constant pool for trace recording with GC root tracking.
//!
//! RPython manages constants implicitly in Trace — ConstPtr boxes are
//! GC-managed objects, so GC can update them when objects move.
//!
//! majit stores Ref constants as raw i64 in a HashMap, invisible to GC.
//! To achieve RPython parity, Ref constants are rooted on the shadow
//! stack (gcreftracer.py:GCREFTRACER parity). GC's walk_roots updates
//! shadow stack entries in place; refresh_from_gc copies updated values
//! back to the HashMap before consumption.

use std::collections::HashMap;

use majit_gc::shadow_stack;
use majit_ir::{GcRef, OpRef, Type};

/// Constant pool for trace recording.
///
/// Manages the mapping from constant-namespace OpRef to i64 values.
/// Each `get_or_insert{,_typed}` call mints a fresh OpRef per
/// `history.py:220/261/307` ConstInt/ConstFloat/ConstPtr `__init__`
/// fresh-allocation; value-equality between two constant OpRefs uses
/// `same_constant` (history.py:204) — not OpRef identity. Resume
/// serialization dedup (`resume.py:148-181 large_ints`/`refs`) is a
/// separate concern that lives in `resume.rs`.
///
/// gcreftracer.py parity: Ref-typed constants are pushed onto the GC
/// shadow stack so that GC can trace and update them if objects move.
/// On consumption (into_inner / snapshot), the HashMap is refreshed
/// from the shadow stack to pick up any GC-updated pointers.
pub struct ConstantPool {
    /// Keyed by OpRef.0 (tagged constant value, i.e. index | CONST_BIT).
    constants: HashMap<u32, i64>,
    /// Type of each constant OpRef. Populated by `get_or_insert_typed`.
    constant_types: HashMap<u32, Type>,
    /// Zero-based counter for allocating new constant indices.
    next_const_idx: u32,
    /// gcreftracer.py parity: (OpRef key, shadow stack index) for each
    /// rooted Ref constant. walk_roots updates shadow stack entries;
    /// refresh_from_gc copies values back to `constants`.
    rooted_refs: Vec<(u32, usize)>,
    /// Shadow stack depth at pool creation. release_roots pops to here.
    shadow_stack_base: usize,
}

impl ConstantPool {
    pub fn new() -> Self {
        ConstantPool {
            constants: HashMap::new(),
            constant_types: HashMap::new(),
            next_const_idx: 0,
            rooted_refs: Vec::new(),
            shadow_stack_base: shadow_stack::depth(),
        }
    }

    /// Mint a fresh ConstInt OpRef for `value`.
    ///
    /// `history.py:220 ConstInt.__init__` allocates a new Box on every
    /// call; equality between two ConstInt boxes for the same value is
    /// `Const.same_constant` (history.py:204/244), not Box identity.
    /// Per-value dedup belongs to the resume tagging memo
    /// (`resume.py:148-172 ResumeDataLoopMemo.large_ints`), which lives
    /// in `resume.rs` and is unaffected by this method.
    ///
    /// `constant_types` records `Type::Int` so callers that reconstruct
    /// the OpRef as `Untyped` (via `OpRef::from_raw`) can still recover
    /// the pinned type. Typed `ConstInt(idx)` variants resolve their
    /// type from the variant tag at priority-0 in `constant_type`.
    pub fn get_or_insert(&mut self, value: i64) -> OpRef {
        let opref = OpRef::const_int(self.next_const_idx);
        self.next_const_idx += 1;
        self.constants.insert(opref.raw(), value);
        self.constant_types.insert(opref.raw(), Type::Int);
        opref
    }

    /// Mint a fresh typed constant OpRef.
    ///
    /// `history.py:220/261/307 ConstInt/ConstFloat/ConstPtr.__init__`
    /// allocate fresh Boxes per call; equality is `Const.same_constant`
    /// (history.py:204). The Ref arm additionally roots the value on
    /// the shadow stack so the GC can update it (gcreftracer.py).
    /// `rooted_refs` (alongside `refresh_from_gc`) is the single source
    /// of truth for which slots track a Ref constant; the post-GC
    /// address is written back to `constants[opref_key]` on each refresh.
    pub fn get_or_insert_typed(&mut self, value: i64, tp: Type) -> OpRef {
        match tp {
            Type::Void => panic!("Void constants are not supported"),
            Type::Float => {
                let opref = OpRef::const_float(self.next_const_idx);
                self.next_const_idx += 1;
                self.constants.insert(opref.raw(), value);
                self.constant_types.insert(opref.raw(), Type::Float);
                opref
            }
            Type::Int => {
                let opref = OpRef::const_int(self.next_const_idx);
                self.next_const_idx += 1;
                self.constants.insert(opref.raw(), value);
                self.constant_types.insert(opref.raw(), Type::Int);
                opref
            }
            Type::Ref => {
                let opref = OpRef::const_ptr(self.next_const_idx);
                self.next_const_idx += 1;
                self.constants.insert(opref.raw(), value);
                self.constant_types.insert(opref.raw(), Type::Ref);
                // gcreftracer.py: non-null Ref constants must be rooted
                // on the shadow stack so the GC can update them when
                // objects move. One root per ConstPtr mint mirrors
                // upstream's per-Box rooting via the ConstPtr.value
                // GCREF reachability.
                if value != 0 {
                    let ss_idx = shadow_stack::push(GcRef(value as usize));
                    self.rooted_refs.push((opref.raw(), ss_idx));
                }
                opref
            }
        }
    }

    /// Get the type of a constant, if recorded.
    ///
    /// Reads the typed OpRef variant tag (ConstInt/ConstFloat/ConstPtr per
    /// history.py:220/261/307) at priority-0 via `opref.ty()`. Falls back
    /// to the `constant_types` side table for Untyped OpRefs (legacy
    /// callers that reconstruct via `OpRef::from_raw`).
    pub fn constant_type(&self, opref: OpRef) -> Option<Type> {
        opref
            .ty()
            .or_else(|| self.constant_types.get(&opref.raw()).copied())
    }

    /// pyjitpl.py:3572 executor.constant_from_op(a) parity:
    /// return the typed Value for a constant OpRef, or None if
    /// the OpRef is not a known constant.
    pub fn get_value(&self, opref: OpRef) -> Option<majit_ir::Value> {
        let &raw = self.constants.get(&opref.raw())?;
        // history.py:220/261/307: ConstInt/Float/Ptr Box.type is pinned at
        // construction. Read the type from the typed OpRef variant tag
        // (priority-0); fall back to the `constant_types` side table for
        // Untyped OpRefs that legacy callers reconstruct via `OpRef::from_raw`.
        let tp = opref
            .ty()
            .or_else(|| self.constant_types.get(&opref.raw()).copied())
            .unwrap_or_else(|| {
                panic!(
                    "constant type missing for OpRef({}) (raw={}) — \
                     get_or_insert / get_or_insert_typed must produce typed \
                     variants or populate constant_types",
                    opref.raw(),
                    raw
                )
            });
        Some(match tp {
            Type::Int => majit_ir::Value::Int(raw),
            Type::Ref => majit_ir::Value::Ref(majit_ir::GcRef(raw as usize)),
            Type::Float => majit_ir::Value::Float(f64::from_bits(raw as u64)),
            Type::Void => majit_ir::Value::Void,
        })
    }

    /// history.py:204 / :244 `Const.same_constant` — true when two
    /// constant OpRefs name the same `(type, value)` pair. Each
    /// `get_or_insert{,_typed}` call mints a fresh OpRef (per
    /// history.py:220/261/307 fresh-alloc), so two ConstInt/ConstFloat/
    /// ConstPtr OpRefs for the same value have distinct raw indices
    /// and compare unequal under `OpRef::eq`. `same_constant` is the
    /// upstream-orthogonal value-equality path; callers comparing
    /// Const boxes for "is this the same value" must use this method
    /// directly.
    ///
    /// Returns `false` for any non-constant OpRef. RPython's
    /// `Const.same_constant` is defined only on the `Const` hierarchy
    /// (history.py:204-208 — base `raise NotImplementedError`); calling
    /// it on a non-constant Box is a type error upstream. We mirror
    /// that contract by short-circuiting non-constants to `false` even
    /// when `a == b` (e.g. `OpRef::NONE == OpRef::NONE`).

    pub fn same_constant(&self, a: OpRef, b: OpRef) -> bool {
        if !a.is_constant() || !b.is_constant() {
            return false;
        }
        if a == b {
            return true;
        }
        // history.py:220/261/307: ConstInt/ConstFloat/ConstPtr are
        // disjoint subclasses; same_constant returns false across them.
        // Read each operand's type via the typed OpRef variant tag at
        // priority-0; fall back to `constant_types` for Untyped operands.
        let a_tp = a
            .ty()
            .or_else(|| self.constant_types.get(&a.raw()).copied());
        let b_tp = b
            .ty()
            .or_else(|| self.constant_types.get(&b.raw()).copied());
        if a_tp != b_tp {
            return false;
        }
        let av = self.constants.get(&a.raw()).copied();
        let bv = self.constants.get(&b.raw()).copied();
        match (av, bv) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    /// Update HashMap from shadow stack — GC may have moved Ref objects.
    /// gcreftracer.py:gcrefs_trace parity.
    ///
    /// `rooted_refs` is populated only by `get_or_insert_typed` under
    /// `tp == Type::Ref`, so every entry here is Ref-typed by
    /// construction (`history.py:307 ConstPtr.type = REF`).
    pub(crate) fn refresh_from_gc(&mut self) {
        for &(opref_key, ss_idx) in &self.rooted_refs {
            let current = shadow_stack::get(ss_idx).0 as i64;
            self.constants.insert(opref_key, current);
        }
    }

    /// Release shadow stack roots.
    /// gcreftracer.py parity: release GC roots for this pool's constants.
    /// XXX majit-only: in RPython, ConstantPool consumption is strictly
    /// LIFO so pop_to always succeeds. In majit, ExportedState may pop
    /// the shadow stack between this pool's creation and release. Until
    /// the LIFO ordering is enforced structurally, guard against this.
    fn release_roots(&mut self) {
        if !self.rooted_refs.is_empty() {
            let current = shadow_stack::depth();
            if current >= self.shadow_stack_base {
                shadow_stack::pop_to(self.shadow_stack_base);
            }
            self.rooted_refs.clear();
        }
    }

    /// Consume the pool and return the constants map.
    pub fn into_inner(mut self) -> HashMap<u32, i64> {
        self.refresh_from_gc();
        let constants = std::mem::take(&mut self.constants);
        self.release_roots();
        constants
    }

    /// Consume the pool, returning both value and type maps.
    pub fn into_inner_with_types(mut self) -> (HashMap<u32, i64>, HashMap<u32, Type>) {
        self.refresh_from_gc();
        let constants = std::mem::take(&mut self.constants);
        let types = std::mem::take(&mut self.constant_types);
        self.release_roots();
        (constants, types)
    }

    /// Consume the pool, returning a typed `HashMap<u32, Value>` that
    /// fuses the legacy `(raw, type)` pair into a single intrinsically-
    /// typed value entry — matching RPython's `Const(value)` Box model
    /// where ConstInt/ConstFloat/ConstPtr (history.py:220/261/307) each
    /// carry their value as a typed instance attribute, not via a
    /// separate type sidetable.
    ///
    /// Convergence path for the cross-crate `constant_types` retirement:
    /// downstream consumers (pyjitpl, trace_ctx, parity, executor, backend)
    /// migrate from `into_inner_with_types` to this typed accessor one at
    /// a time. The legacy accessor stays in place until all consumers
    /// migrate.
    pub fn into_inner_typed(mut self) -> HashMap<u32, majit_ir::Value> {
        self.refresh_from_gc();
        let constants = std::mem::take(&mut self.constants);
        let types = std::mem::take(&mut self.constant_types);
        self.release_roots();
        constants
            .into_iter()
            .map(|(raw, bits)| {
                // history.py:220/261/307 ConstInt/Float/Ptr pin type at
                // construction — every entry MUST have a recorded type.
                // Missing entry => producer didn't populate constant_types
                // in lockstep with constants (writer-side bug); panic loud
                // so the divergence is caught at the seed site.
                let tp = types.get(&raw).copied().unwrap_or_else(|| {
                    panic!(
                        "into_inner_typed: constant_types missing entry for \
                         constant OpRef(raw={raw}) — get_or_insert/get_or_insert_typed \
                         must populate both maps in lockstep"
                    )
                });
                let value = match tp {
                    Type::Int => majit_ir::Value::Int(bits),
                    Type::Float => majit_ir::Value::Float(f64::from_bits(bits as u64)),
                    Type::Ref => majit_ir::Value::Ref(majit_ir::GcRef(bits as usize)),
                    Type::Void => majit_ir::Value::Void,
                };
                (raw, value)
            })
            .collect()
    }

    /// Get a mutable reference to the inner constants map.
    pub fn as_mut(&mut self) -> &mut HashMap<u32, i64> {
        &mut self.constants
    }

    /// Ensure `next_const_idx` is beyond the given const-namespace key.
    /// Used by bridge injection: constants with pre-assigned indices must
    /// not be overwritten by subsequent `get_or_insert` allocations.
    pub fn reserve_index_past(&mut self, opref_key: u32) {
        let raw_idx = opref_key & !(1 << 31);
        self.next_const_idx = self.next_const_idx.max(raw_idx + 1);
    }

    /// Get a shared reference to the inner constants map.
    pub fn as_ref(&self) -> &HashMap<u32, i64> {
        &self.constants
    }

    /// Clone the constants map without consuming the pool.
    /// Refreshes from GC first to pick up moved Ref pointers.
    pub fn snapshot(&mut self) -> HashMap<u32, i64> {
        self.refresh_from_gc();
        self.constants.clone()
    }

    /// Clone the constant type map without consuming the pool.
    pub fn constant_types_snapshot(&self) -> HashMap<u32, Type> {
        self.constant_types.clone()
    }
}

impl Default for ConstantPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ConstantPool {
    fn drop(&mut self) {
        self.release_roots();
    }
}

impl majit_trace::heapcache::SameConstantOracle for ConstantPool {
    fn same_constant(&self, a: OpRef, b: OpRef) -> bool {
        ConstantPool::same_constant(self, a, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_constant_distinct_mints_within_pool_satisfy_predicate() {
        // history.py:204 same_constant on a single ConstantPool: two
        // get_or_insert calls for the same value mint distinct ConstInt
        // OpRefs (history.py:220 fresh-alloc) that satisfy same_constant.
        let mut pool = ConstantPool::new();
        let a = pool.get_or_insert(42);
        let b = pool.get_or_insert(42);
        assert_ne!(a, b, "fresh mints must produce distinct OpRefs");
        assert!(pool.same_constant(a, b));
    }

    #[test]
    fn same_constant_value_aware_across_independent_inserts() {
        // history.py:244 ConstInt.same_constant: two ConstInt instances
        // with the same value compare equal even though they're distinct
        // Box objects in RPython. This helper extends that semantics to
        // pyre OpRefs that may have been minted at different ConstantPool
        // indices (cross-pool deserialisation).
        let mut pool = ConstantPool::new();
        let a = pool.get_or_insert(42);
        // Manually insert a second slot with the same value, bypassing
        // the dedup path (simulates cross-pool composition).
        let b_idx = pool.next_const_idx;
        pool.next_const_idx += 1;
        let b = OpRef::const_int(b_idx);
        pool.constants.insert(b.raw(), 42);
        pool.constant_types.insert(b.raw(), Type::Int);
        assert_ne!(a, b, "different idx slots must be != under variant Eq");
        assert!(
            pool.same_constant(a, b),
            "same_constant must be value-aware",
        );
    }

    #[test]
    fn same_constant_disjoint_subclasses_are_unequal() {
        // history.py:220 / :261 / :307: ConstInt and ConstPtr are
        // disjoint Const subclasses; same_constant returns false across
        // type boundaries even when the underlying value matches.
        let mut pool = ConstantPool::new();
        let i = pool.get_or_insert_typed(0, Type::Int);
        let p = pool.get_or_insert_typed(0, Type::Ref);
        assert_ne!(i, p);
        assert!(!pool.same_constant(i, p));
    }

    #[test]
    fn same_constant_rejects_non_constants() {
        let pool = ConstantPool::new();
        let inputarg = OpRef::input_arg_int(3);
        let op = OpRef::int_op(7);
        assert!(!pool.same_constant(inputarg, op));
        assert!(!pool.same_constant(inputarg, inputarg.with_raw(99)));
    }

    #[test]
    fn same_constant_handles_none() {
        // history.py:204-208 — `Const.same_constant` is defined only on
        // the Const hierarchy. `OpRef::NONE` is not a constant, so the
        // helper must return false even when both operands compare
        // equal under variant-aware Eq.
        let pool = ConstantPool::new();
        assert!(!pool.same_constant(OpRef::NONE, OpRef::NONE));
        assert!(!pool.same_constant(OpRef::NONE, OpRef::const_int(0)));
    }

    /// `get_or_insert(0)` and `get_or_insert_typed(0, Ref)` must NOT alias —
    /// `history.py:220/307` ConstInt(0) and ConstPtr(NULL) are different
    /// classes. Distinct OpRef discriminates the two paths even when the
    /// raw value is identical.
    #[test]
    fn int_ref_zero_does_not_alias() {
        let mut pool = ConstantPool::new();
        let i_zero = pool.get_or_insert(0);
        let r_null = pool.get_or_insert_typed(0, Type::Ref);
        assert_ne!(i_zero, r_null);
        assert_eq!(pool.constant_type(i_zero), Some(Type::Int));
        assert_eq!(pool.constant_type(r_null), Some(Type::Ref));
    }

    /// `history.py:220 ConstInt.__init__` is fresh-alloc per call;
    /// equality between two ConstInt boxes for the same value uses
    /// `Const.same_constant` (history.py:204/244). Pin this contract so
    /// callers that rely on value-equality across two mint operations
    /// migrate to `ConstantPool::same_constant` rather than `OpRef::eq`.
    #[test]
    fn int_value_equality_uses_same_constant_not_eq() {
        let mut pool = ConstantPool::new();
        let a = pool.get_or_insert(42);
        let b = pool.get_or_insert(42);
        assert!(
            pool.same_constant(a, b),
            "two get_or_insert(42) calls must satisfy same_constant"
        );
    }

    /// `history.py:307 ConstPtr.__init__` is fresh-alloc per call;
    /// equality between two ConstPtr boxes for the same address uses
    /// `Const.same_constant`. Mirror the Int pin above for the Ref arm.
    #[test]
    fn ref_value_equality_uses_same_constant_not_eq() {
        let mut pool = ConstantPool::new();
        let a = pool.get_or_insert_typed(0xdead_beef, Type::Ref);
        let b = pool.get_or_insert_typed(0xdead_beef, Type::Ref);
        assert!(
            pool.same_constant(a, b),
            "two get_or_insert_typed(_, Ref) calls must satisfy same_constant"
        );
    }
}
