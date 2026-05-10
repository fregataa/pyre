use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use majit_backend::JitCellToken;
use majit_ir::{CallDescr, DescrRef, EffectInfo, ExtraEffect, OopSpecIndex, Type, VableExpansion};

/// Generic CallDescr for function call operations.
///
/// Stores per-call-site EffectInfo, matching RPython's
/// `effectinfo_from_writeanalyze` (call.py:320).
#[derive(Debug)]
struct MetaCallDescr {
    heapcache_index: u32,
    arg_types: Vec<Type>,
    result_type: Type,
    effect_info: EffectInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EffectInfoKey {
    extraeffect: ExtraEffect,
    oopspecindex: OopSpecIndex,
    readonly_descrs_fields: Option<Vec<u8>>,
    write_descrs_fields: Option<Vec<u8>>,
    readonly_descrs_arrays: Option<Vec<u8>>,
    write_descrs_arrays: Option<Vec<u8>>,
    readonly_descrs_interiorfields: Option<Vec<u8>>,
    write_descrs_interiorfields: Option<Vec<u8>>,
    can_invalidate: bool,
    can_collect: bool,
    call_release_gil_target: (u64, i32),
}

impl EffectInfoKey {
    fn from_effect_info(effect_info: &EffectInfo) -> Self {
        Self {
            extraeffect: effect_info.extraeffect,
            oopspecindex: effect_info.oopspecindex,
            readonly_descrs_fields: effect_info.readonly_descrs_fields.clone(),
            write_descrs_fields: effect_info.write_descrs_fields.clone(),
            readonly_descrs_arrays: effect_info.readonly_descrs_arrays.clone(),
            write_descrs_arrays: effect_info.write_descrs_arrays.clone(),
            readonly_descrs_interiorfields: effect_info.readonly_descrs_interiorfields.clone(),
            write_descrs_interiorfields: effect_info.write_descrs_interiorfields.clone(),
            can_invalidate: effect_info.can_invalidate,
            can_collect: effect_info.can_collect,
            call_release_gil_target: effect_info.call_release_gil_target,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CallDescrKey {
    arg_types: Vec<Type>,
    result_type: Type,
    effect_info: EffectInfoKey,
}

static CALL_DESCR_CACHE: OnceLock<Mutex<HashMap<CallDescrKey, DescrRef>>> = OnceLock::new();
static NEXT_CALL_DESCR_HEAPCACHE_INDEX: AtomicU32 = AtomicU32::new(1_000_000_000);

/// `compile.py:187 isinstance(descr, JitCellToken)` parity.
///
/// RPython's `op.getdescr()` for a `CALL_ASSEMBLER_*` op IS a `JitCellToken`
/// — `record_loop_or_bridge` reads `descr.number` directly and calls
/// `original.record_jump_to(descr)` without any indirection. majit cannot
/// inherit-from the trait, but it preserves the *identity* contract by
/// owning an `Arc<JitCellToken>` here. Callers (`direct_assembler_call`,
/// `compile_tmp_callback`) clone the same Arc that the warm cell /
/// `CompiledEntry::token` /` MemoryManager.alive_loops` already hold, so the
/// keepalive walker's downcast recovers the production token's strong
/// reference rather than a number-recovered side-table lookup.
#[derive(Debug)]
struct MetaCallAssemblerDescr {
    arg_types: Vec<Type>,
    result_type: Type,
    target_token: Arc<JitCellToken>,
    vable_expansion: Option<VableExpansion>,
}

impl majit_ir::Descr for MetaCallDescr {
    fn index(&self) -> u32 {
        self.heapcache_index
    }
    fn as_call_descr(&self) -> Option<&dyn CallDescr> {
        Some(self)
    }
}

impl CallDescr for MetaCallDescr {
    fn arg_types(&self) -> &[Type] {
        &self.arg_types
    }
    fn result_type(&self) -> Type {
        self.result_type
    }
    fn result_size(&self) -> usize {
        0
    }
    fn get_extra_info(&self) -> &EffectInfo {
        &self.effect_info
    }
}

impl majit_ir::Descr for MetaCallAssemblerDescr {
    fn index(&self) -> u32 {
        u32::MAX
    }
    fn as_call_descr(&self) -> Option<&dyn CallDescr> {
        Some(self)
    }
    fn as_loop_token_descr(&self) -> Option<&dyn majit_ir::descr::LoopTokenDescr> {
        Some(self)
    }
}

impl CallDescr for MetaCallAssemblerDescr {
    fn arg_types(&self) -> &[Type] {
        &self.arg_types
    }
    fn result_type(&self) -> Type {
        self.result_type
    }
    fn result_size(&self) -> usize {
        8
    }
    fn call_target_token(&self) -> Option<u64> {
        Some(self.target_token.number)
    }
    fn call_virtualizable_index(&self) -> Option<usize> {
        self.target_token.virtualizable_arg_index
    }
    fn get_extra_info(&self) -> &EffectInfo {
        static INFO: EffectInfo = EffectInfo::const_new(ExtraEffect::CanRaise, OopSpecIndex::None);
        &INFO
    }
    fn vable_expansion(&self) -> Option<&VableExpansion> {
        self.vable_expansion.as_ref()
    }
}

impl majit_ir::descr::LoopTokenDescr for MetaCallAssemblerDescr {
    fn loop_token_number(&self) -> u64 {
        self.target_token.number
    }

    fn call_virtualizable_index(&self) -> Option<usize> {
        self.target_token.virtualizable_arg_index
    }

    fn token_handle_any(&self) -> Option<&dyn std::any::Any> {
        Some(&self.target_token)
    }
}

/// Default EffectInfo for call descriptors that lack per-call-site
/// analysis.
///
/// Upstream `effectinfo_from_writeanalyze` (effectinfo.py:285-298)
/// returns `EF_RANDOM_EFFECTS` (≡ `EffectInfo.MOST_GENERAL`,
/// effectinfo.py:271-273) for any callee whose write-analyzer reports
/// `top_set`. Pyre lacks the analyzer for the residual helpers majit
/// emits today, so the line-by-line match would be `MOST_GENERAL`.
///
/// Two practical caveats keep the default at `EF_CAN_RAISE` with all
/// read/write bitsets full instead:
///
/// 1. `MOST_GENERAL` triggers `OptHeap.call_has_random_effects` which
///    takes the `force_all_lazy_sets + clean_caches` branch. That path
///    correctly flushes the lazy_set described in the comment for
///    `make_call_descr` below — but it also invalidates non-lazy
///    field/array caches and resets `seen_guard_not_invalidated`,
///    which over-zeroes heap state across helper calls in tight loops
///    (visible as 1.5x perf drops on `fib_loop` / `inline_helper`).
/// 2. `MOST_GENERAL` makes `check_forces_virtual_or_virtualizable()`
///    true and the walker tags the call `can_raise = true`, inserting
///    a `GUARD_NO_EXCEPTION` after every helper call. That's a
///    correctness no-op for helpers that never raise but still bloats
///    the trace.
///
/// `EF_CAN_RAISE` analyzer-absent fallback (`effectinfo.py:22`).
/// Upstream `call.py:300 elif self._canraise(op)` selects this
/// extraeffect for plain non-elidable raising callees — the
/// analyzer-absent default for `pyjitpl.py:2007 do_residual_call`'s
/// CALL/CALL_PURE/CALL_MAY_FORCE/CALL_LOOPINVARIANT switch.
/// `MOST_GENERAL` (`EF_RANDOM_EFFECTS = 7`) is reserved for callees
/// the `randomeffects_analyzer` flagged (`call.py:282-283`); using it
/// as a default would route every plain call through
/// `check_forces_virtual_or_virtualizable()` (`pyjitpl.py:2007`,
/// `effectinfo.py:250 extraeffect >= EF_FORCES_VIRTUAL_OR_VIRTUALIZABLE`),
/// inserting unnecessary `GUARD_NOT_FORCED` and over-zeroing heap
/// caches via `force_from_effectinfo`'s `clear_caches` branch.
///
/// PRE-EXISTING-ADAPTATION: same `read/write_descrs_*` and
/// `can_collect` saturation as [`cannot_raise_effect_info()`].
/// `call.py:320-324 effectinfo_from_writeanalyze` builds those
/// bitsets from the `readwrite_analyzer` and `collect_analyzer`
/// results.  Pyre has the analyzer ported (`majit-translate/src/
/// jit_codewriter/call.rs:3250`) but plumbing per-callsite EI from
/// the codewriter pipeline back to runtime trace recording is still
/// pending (Task #64); until that wire-up lands, conservative
/// full-bitset is the line-by-line equivalent of "no analyzer
/// available at this callsite", matching the pre-rebase behaviour
/// `force_from_effectinfo` was already shaped against.
pub fn default_effect_info() -> EffectInfo {
    EffectInfo {
        extraeffect: ExtraEffect::CanRaise,
        oopspecindex: OopSpecIndex::None,
        readonly_descrs_fields: Some(vec![0xff; 8]),
        write_descrs_fields: Some(vec![0xff; 8]),
        readonly_descrs_arrays: Some(vec![0xff; 8]),
        write_descrs_arrays: Some(vec![0xff; 8]),
        readonly_descrs_interiorfields: Some(vec![0xff; 8]),
        write_descrs_interiorfields: Some(vec![0xff; 8]),
        can_invalidate: false,
        can_collect: true,
        single_write_descr_array: None,
        extradescrs: None,
        call_release_gil_target: EffectInfo::_NO_CALL_RELEASE_GIL_TARGET,
    }
}

/// `EF_FORCES_VIRTUAL_OR_VIRTUALIZABLE` analyzer-absent fallback
/// (`effectinfo.py:23`).  Upstream `call.py:288-289` selects this
/// extraeffect when `virtualizable_analyzer.analyze(op)` is true —
/// the explicit "may force" path that `pyjitpl.py:2007-2068` routes
/// through `optimize_CALL_MAY_FORCE_*`, distinct from the
/// `EF_RANDOM_EFFECTS = 7` branch (`call.py:282-283`).  The two
/// values are NOT interchangeable: collapsing onto `MOST_GENERAL`
/// over-claims random-effects semantics on a callee whose only
/// declared escape is virtualizable forcing.
///
/// Same saturated-bitstring rationale as [`default_effect_info()`]:
/// pyre's hand-classified `MayForce` flavor runs without the
/// writeanalyzer, so the conservative full-bitset is the
/// analyzer-absent line-by-line equivalent.
pub fn forces_virtual_or_virtualizable_effect_info() -> EffectInfo {
    EffectInfo {
        extraeffect: ExtraEffect::ForcesVirtualOrVirtualizable,
        oopspecindex: OopSpecIndex::None,
        readonly_descrs_fields: Some(vec![0xff; 8]),
        write_descrs_fields: Some(vec![0xff; 8]),
        readonly_descrs_arrays: Some(vec![0xff; 8]),
        write_descrs_arrays: Some(vec![0xff; 8]),
        readonly_descrs_interiorfields: Some(vec![0xff; 8]),
        write_descrs_interiorfields: Some(vec![0xff; 8]),
        can_invalidate: false,
        can_collect: true,
        single_write_descr_array: None,
        extradescrs: None,
        call_release_gil_target: EffectInfo::_NO_CALL_RELEASE_GIL_TARGET,
    }
}

/// `EF_CANNOT_RAISE` analyzer-absent fallback (`effectinfo.py:19`).
/// Selected by `call.py:303 getcalldescr`'s `else` branch (non-elidable
/// callee whose `_canraise(op) == False`).  `pyjitpl.py:2111-2115
/// do_residual_call` reads `exc = effectinfo.check_can_raise()`
/// (`effectinfo.py:236`) which is false for `extraeffect == 2`, so the
/// canonical walker omits the trailing `GUARD_NO_EXCEPTION`.
///
/// Upstream `effectinfo_from_writeanalyze` (effectinfo.py:276-345)
/// produces `extraeffect == EF_CANNOT_RAISE` only in the analyzed
/// branch, where the bitstrings start as empty `[]` and grow with the
/// per-callee analyzer outputs.  Pyre hand-classifies a callee as
/// `CannotRaise` (`#[dont_look_inside_cannot_raise]` /
/// `CallFlavor::PlainCannotRaise`) without running the writeanalyzer,
/// so the analyzer-absent shape must be conservative on heap effects:
/// the saturated `Some(vec![0xff; 8])` mask matches the same per-cached-
/// descr flush behaviour `default_effect_info()` historically used for
/// raising callees, only with `extraeffect == CannotRaise` so
/// `pyjitpl.py:2111-2115` still omits `GUARD_NO_EXCEPTION`.  Upstream
/// never produces this third "EF_CANNOT_RAISE + saturated mask" shape
/// because PyPy always runs the analyzer; PRE-EXISTING-ADAPTATION on
/// the unanalyzed hand-classification itself.  Helpers known to touch
/// no heap should opt into [`CANNOT_RAISE_NO_HEAP_EFFECT_INFO`]
/// (the analyzer-output empty-set) instead.
pub fn cannot_raise_effect_info() -> EffectInfo {
    EffectInfo {
        extraeffect: ExtraEffect::CannotRaise,
        oopspecindex: OopSpecIndex::None,
        readonly_descrs_fields: Some(vec![0xff; 8]),
        write_descrs_fields: Some(vec![0xff; 8]),
        readonly_descrs_arrays: Some(vec![0xff; 8]),
        write_descrs_arrays: Some(vec![0xff; 8]),
        readonly_descrs_interiorfields: Some(vec![0xff; 8]),
        write_descrs_interiorfields: Some(vec![0xff; 8]),
        can_invalidate: false,
        can_collect: true,
        single_write_descr_array: None,
        extradescrs: None,
        call_release_gil_target: EffectInfo::_NO_CALL_RELEASE_GIL_TARGET,
    }
}

/// `EF_CANNOT_RAISE` for a callee that the producer statically knows
/// touches no heap state and cannot trigger GC — typically a flat TLS
/// read/write or a buffer-flush shim.  `call.py:320-324
/// effectinfo_from_writeanalyze` would compute empty
/// `readonly_descrs_*` / `write_descrs_*` bitsets and `can_collect =
/// False` from `read_analyzer` / `write_analyzer` / `collect_analyzer`
/// for such helpers.  Using [`cannot_raise_effect_info()`] for them is
/// the analyzer-absent conservative fallback, which over-reports the
/// callee as a heap mutator and inflates GC map / liveness work; this
/// constant is the matching analyzer-output for known-flat helpers.
pub const CANNOT_RAISE_NO_HEAP_EFFECT_INFO: EffectInfo = EffectInfo {
    extraeffect: ExtraEffect::CannotRaise,
    oopspecindex: OopSpecIndex::None,
    readonly_descrs_fields: Some(Vec::new()),
    write_descrs_fields: Some(Vec::new()),
    readonly_descrs_arrays: Some(Vec::new()),
    write_descrs_arrays: Some(Vec::new()),
    readonly_descrs_interiorfields: Some(Vec::new()),
    write_descrs_interiorfields: Some(Vec::new()),
    can_invalidate: false,
    can_collect: false,
    single_write_descr_array: None,
    extradescrs: None,
    call_release_gil_target: EffectInfo::_NO_CALL_RELEASE_GIL_TARGET,
};

/// `EF_ELIDABLE_CANNOT_RAISE` with `OS_INT_PY_DIV` oopspec — Python `//`
/// (floor division). RPython parity: jtransform.py:2046-2047
/// `_handle_int_special` classifies `int.py_div` as
/// `EF_ELIDABLE_CANNOT_RAISE`. Source-level zero/overflow wrappers
/// (`rint.py:417 ll_int_py_div_zer`, `:429 ll_int_py_div_ovf_zer`)
/// are inlined into the calling graph before the JIT sees this
/// oopspec call; their checks become runtime guards in the trace,
/// not properties of this call descriptor. The optimizer's
/// `optimize_call_int_py_div` (rewrite.py:713-766) reads the
/// `OS_INT_PY_DIV` oopspec to specialize power-of-2 divisors to
/// `int_rshift`, constant 1 to identity, constant -1 to `int_neg`, etc.
/// Callee is pure: no heap touched, no GC trigger, no raise.
pub const INT_PY_DIV_EFFECT_INFO: EffectInfo = EffectInfo {
    extraeffect: ExtraEffect::ElidableCannotRaise,
    oopspecindex: OopSpecIndex::IntPyDiv,
    readonly_descrs_fields: Some(Vec::new()),
    write_descrs_fields: Some(Vec::new()),
    readonly_descrs_arrays: Some(Vec::new()),
    write_descrs_arrays: Some(Vec::new()),
    readonly_descrs_interiorfields: Some(Vec::new()),
    write_descrs_interiorfields: Some(Vec::new()),
    can_invalidate: false,
    can_collect: false,
    single_write_descr_array: None,
    extradescrs: None,
    call_release_gil_target: EffectInfo::_NO_CALL_RELEASE_GIL_TARGET,
};

/// Counterpart of [`INT_PY_DIV_EFFECT_INFO`] for Python `%`. RPython
/// parity: jtransform.py:2046-2047 classifies `int.py_mod` as
/// `EF_ELIDABLE_CANNOT_RAISE`; zero/overflow checks from the source
/// wrappers (`rint.py:509 ll_int_py_mod_zer`, `:520
/// ll_int_py_mod_ovf_zer`) are inlined upstream of the JIT trace.
pub const INT_PY_MOD_EFFECT_INFO: EffectInfo = EffectInfo {
    extraeffect: ExtraEffect::ElidableCannotRaise,
    oopspecindex: OopSpecIndex::IntPyMod,
    readonly_descrs_fields: Some(Vec::new()),
    write_descrs_fields: Some(Vec::new()),
    readonly_descrs_arrays: Some(Vec::new()),
    write_descrs_arrays: Some(Vec::new()),
    readonly_descrs_interiorfields: Some(Vec::new()),
    write_descrs_interiorfields: Some(Vec::new()),
    can_invalidate: false,
    can_collect: false,
    single_write_descr_array: None,
    extradescrs: None,
    call_release_gil_target: EffectInfo::_NO_CALL_RELEASE_GIL_TARGET,
};

/// `EF_ELIDABLE_CANNOT_RAISE` (effectinfo.py:17). Selected by
/// `call.py:299 getcalldescr` when `_canraise(op) == False` for an
/// elidable callee — `pyjitpl.py:2126 do_residual_call` records
/// `CALL_PURE_*` without the trailing `GUARD_NO_EXCEPTION` because
/// `effectinfo.check_can_raise()` (`effectinfo.py:232`) is false for
/// `extraeffect == 0`.
pub const ELIDABLE_CANNOT_RAISE_EFFECT_INFO: EffectInfo =
    EffectInfo::const_new(ExtraEffect::ElidableCannotRaise, OopSpecIndex::None);

/// `EF_ELIDABLE_OR_MEMORYERROR` (effectinfo.py:20). Selected by
/// `call.py:295 getcalldescr` when `_canraise(op) == "mem"` — i.e.
/// the elidable callee's only failure mode is `MemoryError`. Same
/// dispatch as `EF_ELIDABLE_CAN_RAISE` (`check_can_raise()` is true
/// for extraeffect ≥ 3) but distinguishes memory-only raises for the
/// optimizer.
pub const ELIDABLE_OR_MEMERROR_EFFECT_INFO: EffectInfo =
    EffectInfo::const_new(ExtraEffect::ElidableOrMemoryError, OopSpecIndex::None);

/// `EF_ELIDABLE_CAN_RAISE` (effectinfo.py:21). Pure calls do not need
/// the conservative flush — `effectinfo_from_writeanalyze` (effectinfo.py:
/// 169-181) clears `_write_descrs_*` for elidable extraeffects. With
/// the bitsets at zero this becomes "no writes" inside
/// `force_from_effectinfo`, matching upstream.
pub const ELIDABLE_EFFECT_INFO: EffectInfo =
    EffectInfo::const_new(ExtraEffect::ElidableCanRaise, OopSpecIndex::None);

/// `EF_LOOPINVARIANT` (effectinfo.py:18). Same write-mask treatment as
/// elidable; the trace optimizer recognises the opcode and skips cache
/// invalidation regardless of the bitsets.
pub const LOOPINVARIANT_EFFECT_INFO: EffectInfo =
    EffectInfo::const_new(ExtraEffect::LoopInvariant, OopSpecIndex::None);

/// Per-callee analyzer-result slot.  Mirrors `call.py:282-303 getcalldescr`'s
/// `extraeffect` selection without the `raise_analyzer` /
/// `readwrite_analyzer` / `collect_analyzer` / `randomeffects_analyzer`
/// graph-based machinery (the analyzers operate on RPython low-level
/// graphs, which pyre does not have).  Producers that statically know
/// the callee's classification — typically because the helper carries
/// a `#[elidable]` / `#[elidable_cannot_raise]` / `#[dont_look_inside]`
/// attribute — pick the matching slot at registration time;
/// [`effect_info_for_slot`] resolves it to the corresponding
/// [`EffectInfo`] const at descr construction.
///
/// `MayForce` (`EF_FORCES_VIRTUAL_OR_VIRTUALIZABLE`) and `ReleaseGil`
/// (`EF_RANDOM_EFFECTS` + non-zero `call_release_gil_target`) are
/// deliberately omitted — those EI values carry runtime-resolved
/// `target.concrete_ptr` / `save_err` slots that the const factory at
/// `jitcode/assembler.rs:emit_canonical_call_*_via_target` constructs
/// inline.  Adding them here would require duplicating the
/// `(1, 0)` sentinel + `resolve_call_release_gil_target` substitution,
/// which is out of scope for the slot enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EffectInfoSlot {
    /// `EF_CAN_RAISE` — `call.py:301` `elif self._canraise(op)`.
    /// Conservative analyzer-absent default; matches `default_effect_info()`.
    #[default]
    CanRaise,
    /// `EF_CANNOT_RAISE` — `call.py:303` `else` branch.
    CannotRaise,
    /// `EF_ELIDABLE_CAN_RAISE` — `call.py:297` `elif cr:` branch.
    ElidableCanRaise,
    /// `EF_ELIDABLE_CANNOT_RAISE` — `call.py:299` `else` branch under
    /// `elif elidable:`.
    ElidableCannotRaise,
    /// `EF_ELIDABLE_OR_MEMORYERROR` — `call.py:295` `if cr == "mem":`.
    ElidableOrMemerror,
    /// `EF_LOOPINVARIANT` — `call.py:291` `elif loopinvariant:`.
    LoopInvariant,
}

/// Resolve a [`EffectInfoSlot`] to its matching [`EffectInfo`] const.
///
/// `call.py:320 effectinfo_from_writeanalyze` constructs the final EI
/// from the `extraeffect` plus the analyzer outputs; pyre's per-slot
/// const captures the analyzer-absent fallback for that `extraeffect`.
pub fn effect_info_for_slot(slot: EffectInfoSlot) -> EffectInfo {
    match slot {
        EffectInfoSlot::CanRaise => default_effect_info(),
        EffectInfoSlot::CannotRaise => cannot_raise_effect_info(),
        EffectInfoSlot::ElidableCanRaise => ELIDABLE_EFFECT_INFO,
        EffectInfoSlot::ElidableCannotRaise => ELIDABLE_CANNOT_RAISE_EFFECT_INFO,
        EffectInfoSlot::ElidableOrMemerror => ELIDABLE_OR_MEMERROR_EFFECT_INFO,
        EffectInfoSlot::LoopInvariant => LOOPINVARIANT_EFFECT_INFO,
    }
}

/// Pick the upstream-equivalent default effect for an opcode whose
/// callee has not been write-analyzed.
///
/// `pyjitpl.py:1991-1995 do_residual_or_indirect_call` selects between
/// CALL / CALL_PURE / CALL_LOOPINVARIANT / CALL_MAY_FORCE based on
/// `descr.get_extra_info().extraeffect`. Pyre baked the choice into the
/// opcode at codewriter time, so reverse the mapping here so the descr
/// the optimizer reads carries the matching effect class.
pub fn default_effect_for_opcode(opcode: majit_ir::OpCode) -> EffectInfo {
    if opcode.is_call_pure() {
        ELIDABLE_EFFECT_INFO
    } else if opcode.is_call_loopinvariant() {
        LOOPINVARIANT_EFFECT_INFO
    } else {
        default_effect_info()
    }
}

/// Create a CallDescr with the conservative
/// [`default_effect_info()`] (`EF_CAN_RAISE` + saturated
/// `Some(vec![0xff; 8])` field/array bitsets).  This is the
/// analyzer-absent fallback mirroring `call.py:300 elif self._canraise(op)`
/// — the extraeffect upstream picks for plain non-elidable raising
/// callees whenever `call.py:296-326 getcalldescr` runs without
/// per-callsite analyzer output.
///
/// Production producers should prefer one of the more specific factories
/// so the per-callee classification reaches the trace IR:
///
/// * [`make_call_descr_from_target_slot`] when a resolved
///   [`crate::jitcode::JitCallTarget`] is available — threads the
///   macro-time [`EffectInfoSlot`] (`call.py:282-303 getcalldescr` parity).
/// * [`make_call_descr_for_opcode`] when only the call opcode family is
///   known (`pyjitpl.py:1991-1995 do_residual_or_indirect_call`'s
///   `EF_LOOPINVARIANT` / `EF_ELIDABLE_*` reverse-mapping).
/// * [`make_call_descr_with_effect`] when an explicit `EffectInfo` has
///   been hand-built (release-gil targets, oopspec specializations).
///
/// Remaining direct callers of this fallback are restricted to
/// `#[cfg(test)]` fixtures (pyjitpl/optimizeopt/backend test stubs)
/// where the conservative descr is the test's intent — matching the
/// "no analyzer ran" path the production fallbacks above subsume.
pub fn make_call_descr(arg_types: &[Type], result_type: Type) -> DescrRef {
    make_call_descr_with_effect(arg_types, result_type, default_effect_info())
}

/// Create a CallDescr whose effect info matches the call opcode family.
pub fn make_call_descr_for_opcode(
    opcode: majit_ir::OpCode,
    arg_types: &[Type],
    result_type: Type,
) -> DescrRef {
    make_call_descr_with_effect(arg_types, result_type, default_effect_for_opcode(opcode))
}

/// Create a CallDescr from a per-target [`EffectInfoSlot`] classification.
///
/// `call.py:282-303 getcalldescr` selects `extraeffect` per callsite
/// from the analyzer chain; pyre's analyzer-absent equivalent is the
/// `JitCallTarget.effect_info_slot` macro-time classification.  This
/// factory is the per-target entry point — callers that have a
/// resolved [`crate::jitcode::JitCallTarget`] thread its slot through.
pub fn make_call_descr_from_target_slot(
    arg_types: &[Type],
    result_type: Type,
    slot: EffectInfoSlot,
) -> DescrRef {
    make_call_descr_with_effect(arg_types, result_type, effect_info_for_slot(slot))
}

/// call.py:320 `effectinfo_from_writeanalyze` parity. Create a
/// CallDescr with explicit per-call-site EffectInfo.
pub fn make_call_descr_with_effect(
    arg_types: &[Type],
    result_type: Type,
    effect_info: EffectInfo,
) -> DescrRef {
    // effectinfo.py:144-146: `if tgt_func: key += (object(),)  # don't
    // care about caching in this case` — release-gil targets bypass the
    // EffectInfo._cache because each one carries a unique target
    // function pointer that the deduplicator should not collapse.
    // Mirror by short-circuiting the cache lookup/insert when the
    // release-gil target is non-null.
    if effect_info.call_release_gil_target.0 != 0 {
        return Arc::new(MetaCallDescr {
            heapcache_index: NEXT_CALL_DESCR_HEAPCACHE_INDEX.fetch_add(1, Ordering::Relaxed),
            arg_types: arg_types.to_vec(),
            result_type,
            effect_info,
        });
    }

    let key = CallDescrKey {
        arg_types: arg_types.to_vec(),
        result_type,
        effect_info: EffectInfoKey::from_effect_info(&effect_info),
    };

    // descr.py:22 `GcCache._cache_call`: call descriptors are cached
    // structurally, so repeated construction of the same call shape
    // yields the same descr identity.  The HashMap is that RPython
    // descriptor cache, not a side table for per-box optimizer state.
    let cache = CALL_DESCR_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap();
    if let Some(descr) = cache.get(&key) {
        return descr.clone();
    }
    let descr: DescrRef = Arc::new(MetaCallDescr {
        heapcache_index: NEXT_CALL_DESCR_HEAPCACHE_INDEX.fetch_add(1, Ordering::Relaxed),
        arg_types: arg_types.to_vec(),
        result_type,
        effect_info,
    });
    cache.insert(key, descr.clone());
    descr
}

/// Create a CallDescr for CALL_MAY_FORCE_* operations.
///
/// RPython treats these as may-raise calls guarded by GUARD_NOT_FORCED, not as
/// generic cannot-raise helpers.
pub fn make_call_may_force_descr(arg_types: &[Type], result_type: Type) -> DescrRef {
    #[derive(Debug)]
    struct MetaCallMayForceDescr {
        arg_types: Vec<Type>,
        result_type: Type,
    }

    impl majit_ir::Descr for MetaCallMayForceDescr {
        fn index(&self) -> u32 {
            u32::MAX
        }
        fn as_call_descr(&self) -> Option<&dyn CallDescr> {
            Some(self)
        }
    }

    impl CallDescr for MetaCallMayForceDescr {
        fn arg_types(&self) -> &[Type] {
            &self.arg_types
        }
        fn result_type(&self) -> Type {
            self.result_type
        }
        fn result_size(&self) -> usize {
            0
        }
        fn get_extra_info(&self) -> &EffectInfo {
            // CALL_MAY_FORCE pairs with `GUARD_NOT_FORCED`; the
            // optimizer postpones the call (heap.rs:2722-2747) and
            // flushes lazy sets at the guard via
            // `force_lazy_sets_for_guard` (heap.rs:2770). That's the
            // single flush that mirrors RPython's same code path, so
            // there is no need to also fire `force_from_effectinfo`
            // at the call site itself — leave the bitsets empty.
            // `EF_CAN_RAISE` keeps the optimizer from flagging the
            // call as elidable / loopinvariant.
            static INFO: EffectInfo =
                EffectInfo::const_new(ExtraEffect::CanRaise, OopSpecIndex::None);
            &INFO
        }
    }

    Arc::new(MetaCallMayForceDescr {
        arg_types: arg_types.to_vec(),
        result_type,
    })
}

/// `compile.py:187 isinstance(descr, JitCellToken)` parity factory.
///
/// Create a `CALL_ASSEMBLER_*` descr that owns the same `Arc<JitCellToken>`
/// as the production warm cell / `CompiledEntry::token` / `alive_loops`.
/// `direct_assembler_call` (`pyjitpl.py:3589-3609`) is the canonical caller —
/// it threads the cell's compiled token through, so `record_loop_or_bridge`'s
/// keepalive walker downcasts the descr and pushes that same Arc into
/// `original.keepalive_tokens`, matching `compile.py:187 record_jump_to(descr)`.
pub fn make_call_assembler_descr(
    target_token: Arc<JitCellToken>,
    arg_types: &[Type],
    result_type: Type,
) -> DescrRef {
    Arc::new(MetaCallAssemblerDescr {
        arg_types: arg_types.to_vec(),
        result_type,
        target_token,
        vable_expansion: None,
    })
}

/// Number-only factory for callers that have not yet been threaded an
/// `Arc<JitCellToken>` (jitcode dispatch in `dispatch.rs`, test fixtures).
///
/// Synthesises a fresh stand-alone `Arc<JitCellToken>` with the requested
/// `target_number` so the descr keeps the same shape as the identity-preserving
/// path. Identity is **not** preserved — the keepalive walker recovers the
/// real Arc via `jitcell_token_by_number(target_number)` for these descrs
/// (`pyjitpl/mod.rs:record_loop_or_bridge` Arc-fallback inside the
/// CALL_ASSEMBLER branch). Sites transitioning to
/// `make_call_assembler_descr` once the Arc is available upstream remove
/// the lookup.
pub fn make_call_assembler_descr_by_number(
    target_number: u64,
    arg_types: &[Type],
    result_type: Type,
    virtualizable_arg_index: Option<usize>,
) -> DescrRef {
    let mut tok = JitCellToken::new(target_number);
    tok.virtualizable_arg_index = virtualizable_arg_index;
    make_call_assembler_descr(Arc::new(tok), arg_types, result_type)
}

/// rewrite.py:665-695 handle_call_assembler: create a CallDescr that carries
/// virtualizable expansion info. The backend reads fields from the frame
/// reference to populate the callee's full inputarg jitframe layout.
pub fn make_call_assembler_descr_with_vable(
    target_token: Arc<JitCellToken>,
    arg_types: &[Type],
    result_type: Type,
    expansion: VableExpansion,
) -> DescrRef {
    Arc::new(MetaCallAssemblerDescr {
        arg_types: arg_types.to_vec(),
        result_type,
        target_token,
        vable_expansion: Some(expansion),
    })
}

/// Number-only sibling of `make_call_assembler_descr_with_vable` for transitional
/// callers (jitcode dispatch). See `make_call_assembler_descr_by_number`.
pub fn make_call_assembler_descr_with_vable_by_number(
    target_number: u64,
    arg_types: &[Type],
    result_type: Type,
    expansion: VableExpansion,
) -> DescrRef {
    let tok = Arc::new(JitCellToken::new(target_number));
    make_call_assembler_descr_with_vable(tok, arg_types, result_type, expansion)
}
