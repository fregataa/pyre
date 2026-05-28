//! `rpython/rtyper/rweakref.py` — RTyping of RPython-level weakrefs.

use std::fmt;
use std::sync::Arc;

use crate::flowspace::model::{ConstValue, Constant, Hlvalue};
use crate::translator::rtyper::error::TyperError;
use crate::translator::rtyper::lltypesystem::lltype::{
    FrozenDict, GcKind, LowLevelType, MallocFlavor, OpaqueType, Ptr, PtrTarget, StructType,
    WEAKREF, WEAKREF_PTR, malloc, nullptr, opaqueptr,
};
use crate::translator::rtyper::pairtype::ReprClassId;
use crate::translator::rtyper::rmodel::{
    RTypeResult, Repr, ReprState, gc_flavor_const, lowlevel_type_const,
};
use crate::translator::rtyper::rtyper::{ConvertedTo, GenopResult, HighLevelOp, RPythonTyper};

// ─── BaseWeakRefRepr trait ───────────────────────────────────────────

/// rweakref.py:23-48 `class BaseWeakRefRepr(Repr)`.
///
/// Shared interface for `WeakRefRepr` (native) and
/// `EmulatedWeakRefRepr` (rweakref=False fallback).
pub trait BaseWeakRefReprTrait: Repr {
    /// rweakref.py:58/80 `_weakref_create(hop, v_inst)`.
    fn weakref_create(&self, hop: &HighLevelOp, v_inst: Hlvalue) -> RTypeResult;

    /// rweakref.py:62/91 `_weakref_deref(hop, v_wref)`.
    fn weakref_deref(&self, hop: &HighLevelOp, v_wref: Hlvalue) -> RTypeResult;
}

/// Test whether a `&dyn Repr` is a `BaseWeakRefRepr` subclass.
pub fn is_base_weakref_repr(r: &dyn Repr) -> bool {
    let any_r: &dyn std::any::Any = r;
    any_r.downcast_ref::<WeakRefRepr>().is_some()
        || any_r.downcast_ref::<EmulatedWeakRefRepr>().is_some()
}

/// Downcast `&dyn Repr` to `&dyn BaseWeakRefReprTrait`.
pub fn as_base_weakref_repr(r: &dyn Repr) -> Option<&dyn BaseWeakRefReprTrait> {
    let any_r: &dyn std::any::Any = r;
    if let Some(w) = any_r.downcast_ref::<WeakRefRepr>() {
        return Some(w as &dyn BaseWeakRefReprTrait);
    }
    if let Some(e) = any_r.downcast_ref::<EmulatedWeakRefRepr>() {
        return Some(e as &dyn BaseWeakRefReprTrait);
    }
    None
}

fn ptr_target_as_lowleveltype(target: &PtrTarget) -> LowLevelType {
    match target {
        PtrTarget::Func(t) => LowLevelType::Func(Box::new(t.clone())),
        PtrTarget::Struct(t) => LowLevelType::Struct(Box::new(t.clone())),
        PtrTarget::Array(t) => LowLevelType::Array(Box::new(t.clone())),
        PtrTarget::FixedSizeArray(t) => LowLevelType::FixedSizeArray(Box::new(t.clone())),
        PtrTarget::Opaque(t) => LowLevelType::Opaque(Box::new(t.clone())),
        PtrTarget::ForwardReference(t) => LowLevelType::ForwardReference(Box::new(t.clone())),
    }
}

fn ptr_pointee_type(ptr_type: &LowLevelType) -> Result<LowLevelType, TyperError> {
    let LowLevelType::Ptr(ptr) = ptr_type else {
        return Err(TyperError::message(format!(
            "weakref repr lowleveltype must be Ptr(...), got {ptr_type:?}",
        )));
    };
    Ok(ptr_target_as_lowleveltype(&ptr.TO))
}

fn null_ptr_constant(ptr_type: &LowLevelType) -> Result<Constant, TyperError> {
    let pointee = ptr_pointee_type(ptr_type)?;
    let ptr = nullptr(pointee).map_err(TyperError::message)?;
    Ok(Constant::with_concretetype(
        ConstValue::LLPtr(Box::new(ptr)),
        ptr_type.clone(),
    ))
}

fn native_dead_wref_constant() -> Result<Constant, TyperError> {
    let ptr = opaqueptr(WEAKREF.clone(), "dead_wref").map_err(TyperError::message)?;
    Ok(Constant::with_concretetype(
        ConstValue::LLPtr(Box::new(ptr)),
        WEAKREF_PTR.clone(),
    ))
}

fn emulated_dead_wref_constant(ptr_type: &LowLevelType) -> Result<Constant, TyperError> {
    let pointee = ptr_pointee_type(ptr_type)?;
    let ptr = malloc(pointee, None, MallocFlavor::Gc, true).map_err(TyperError::message)?;
    Ok(Constant::with_concretetype(
        ConstValue::LLPtr(Box::new(ptr)),
        ptr_type.clone(),
    ))
}

fn gcref_type() -> LowLevelType {
    LowLevelType::Ptr(Box::new(Ptr {
        TO: PtrTarget::Opaque(OpaqueType::gc("GCREF")),
    }))
}

fn ref_field_const() -> Result<Constant, TyperError> {
    HighLevelOp::inputconst(&LowLevelType::Void, &ConstValue::byte_str("ref"))
}

// ─── WeakRefRepr ─────────────────────────────────────────────────────

/// rweakref.py:51-64 `class WeakRefRepr(BaseWeakRefRepr)`.
/// `lowleveltype = WeakRefPtr`.
#[derive(Debug)]
pub struct WeakRefRepr {
    state: ReprState,
}

impl WeakRefRepr {
    pub fn new() -> Self {
        WeakRefRepr {
            state: ReprState::new(),
        }
    }
}

impl Repr for WeakRefRepr {
    fn lowleveltype(&self) -> &LowLevelType {
        &WEAKREF_PTR
    }

    fn state(&self) -> &ReprState {
        &self.state
    }

    fn class_name(&self) -> &'static str {
        "WeakRefRepr"
    }

    fn repr_class_id(&self) -> ReprClassId {
        ReprClassId::WeakRefRepr
    }

    /// rweakref.py:27-39
    fn convert_const(&self, value: &ConstValue) -> Result<Constant, TyperError> {
        match value {
            ConstValue::None => null_ptr_constant(self.lowleveltype()),
            ConstValue::HostObject(obj) if obj.is_weakref() => match obj.weakref_referent() {
                Some(None) => native_dead_wref_constant(),
                Some(Some(_)) => Err(TyperError::message(
                    "WeakRefRepr.convert_const: live prebuilt weakref conversion requires \
                     rtyper.bindingrepr(Constant(instance))",
                )),
                None => unreachable!("is_weakref checked above"),
            },
            _ => Err(TyperError::message(format!(
                "WeakRefRepr.convert_const: expected None or weakref.ref, got {value:?}",
            ))),
        }
    }

    /// rweakref.py:41-48
    fn rtype_simple_call(&self, hop: &HighLevelOp) -> RTypeResult {
        let vlist = hop.inputargs(vec![ConvertedTo::Repr(self)])?;
        hop.exception_cannot_occur()?;
        let r_result = hop.r_result.borrow();
        let r_result = r_result
            .as_ref()
            .ok_or_else(|| TyperError::message("WeakRefRepr.rtype_simple_call: no r_result"))?;
        if *r_result.lowleveltype() == LowLevelType::Void {
            Ok(Some(Hlvalue::Constant(Constant::with_concretetype(
                ConstValue::None,
                LowLevelType::Void,
            ))))
        } else {
            self.weakref_deref(hop, vlist.into_iter().next().unwrap())
        }
    }
}

impl BaseWeakRefReprTrait for WeakRefRepr {
    /// rweakref.py:58-60
    fn weakref_create(&self, hop: &HighLevelOp, v_inst: Hlvalue) -> RTypeResult {
        Ok(hop.genop(
            "weakref_create",
            vec![v_inst],
            GenopResult::LLType(WEAKREF_PTR.clone()),
        ))
    }

    /// rweakref.py:62-64
    fn weakref_deref(&self, hop: &HighLevelOp, v_wref: Hlvalue) -> RTypeResult {
        let r_result = hop.r_result.borrow();
        let r_result = r_result
            .as_ref()
            .ok_or_else(|| TyperError::message("WeakRefRepr._weakref_deref: no r_result"))?;
        Ok(hop.genop(
            "weakref_deref",
            vec![v_wref],
            GenopResult::Repr(r_result.clone()),
        ))
    }
}

impl fmt::Display for WeakRefRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.repr_string())
    }
}

// ─── EmulatedWeakRefRepr ─────────────────────────────────────────────

/// rweakref.py:67-96 `class EmulatedWeakRefRepr(BaseWeakRefRepr)`.
/// For `rweakref=False`, emulates weakrefs with strong references.
/// `lowleveltype = Ptr(GcStruct('EmulatedWeakRef', ('ref', GCREF)))`.
#[derive(Debug)]
pub struct EmulatedWeakRefRepr {
    state: ReprState,
    lltype: LowLevelType,
}

/// `Ptr(GcStruct('EmulatedWeakRef', ('ref', GCREF)))` (rweakref.py:71-72).
fn emulated_weakref_lltype() -> LowLevelType {
    let gcref = LowLevelType::Ptr(Box::new(Ptr {
        TO: PtrTarget::Opaque(OpaqueType::gc("GCREF")),
    }));
    let flds = FrozenDict::new(vec![("ref".into(), gcref)]);
    LowLevelType::Ptr(Box::new(Ptr {
        TO: PtrTarget::Struct(StructType {
            _name: "EmulatedWeakRef".into(),
            _flds: flds,
            _names: vec!["ref".into()],
            _adtmeths: FrozenDict::new(vec![]),
            _hints: FrozenDict::new(vec![]),
            _arrayfld: None,
            _gckind: GcKind::Gc,
            _runtime_type_info: None,
        }),
    }))
}

impl EmulatedWeakRefRepr {
    pub fn new() -> Self {
        EmulatedWeakRefRepr {
            state: ReprState::new(),
            lltype: emulated_weakref_lltype(),
        }
    }
}

impl Repr for EmulatedWeakRefRepr {
    fn lowleveltype(&self) -> &LowLevelType {
        &self.lltype
    }

    fn state(&self) -> &ReprState {
        &self.state
    }

    fn class_name(&self) -> &'static str {
        "EmulatedWeakRefRepr"
    }

    fn repr_class_id(&self) -> ReprClassId {
        ReprClassId::EmulatedWeakRefRepr
    }

    /// rweakref.py:27-39
    fn convert_const(&self, value: &ConstValue) -> Result<Constant, TyperError> {
        match value {
            ConstValue::None => null_ptr_constant(&self.lltype),
            ConstValue::HostObject(obj) if obj.is_weakref() => match obj.weakref_referent() {
                Some(None) => emulated_dead_wref_constant(&self.lltype),
                Some(Some(_)) => Err(TyperError::message(
                    "EmulatedWeakRefRepr.convert_const: live prebuilt weakref conversion requires \
                     rtyper.bindingrepr(Constant(instance))",
                )),
                None => unreachable!("is_weakref checked above"),
            },
            _ => Err(TyperError::message(format!(
                "EmulatedWeakRefRepr.convert_const: expected None or weakref.ref, got {value:?}",
            ))),
        }
    }

    /// rweakref.py:41-48
    fn rtype_simple_call(&self, hop: &HighLevelOp) -> RTypeResult {
        let vlist = hop.inputargs(vec![ConvertedTo::Repr(self)])?;
        hop.exception_cannot_occur()?;
        let r_result = hop.r_result.borrow();
        let r_result = r_result.as_ref().ok_or_else(|| {
            TyperError::message("EmulatedWeakRefRepr.rtype_simple_call: no r_result")
        })?;
        if *r_result.lowleveltype() == LowLevelType::Void {
            Ok(Some(Hlvalue::Constant(Constant::with_concretetype(
                ConstValue::None,
                LowLevelType::Void,
            ))))
        } else {
            self.weakref_deref(hop, vlist.into_iter().next().unwrap())
        }
    }
}

impl BaseWeakRefReprTrait for EmulatedWeakRefRepr {
    /// rweakref.py:80-89
    fn weakref_create(&self, hop: &HighLevelOp, v_inst: Hlvalue) -> RTypeResult {
        let c_type = lowlevel_type_const(ptr_pointee_type(&self.lltype)?);
        let c_flags = gc_flavor_const()?;
        let v_ptr = hop
            .genop(
                "malloc",
                vec![c_type, c_flags],
                GenopResult::LLType(self.lltype.clone()),
            )
            .ok_or_else(|| TyperError::message("EmulatedWeakRefRepr: malloc genop failed"))?;
        let v_gcref = hop
            .genop(
                "cast_opaque_ptr",
                vec![v_inst],
                GenopResult::LLType(gcref_type()),
            )
            .ok_or_else(|| {
                TyperError::message("EmulatedWeakRefRepr: cast_opaque_ptr genop failed")
            })?;
        let c_ref = ref_field_const()?;
        hop.genop(
            "setfield",
            vec![v_ptr.clone(), Hlvalue::Constant(c_ref), v_gcref],
            GenopResult::Void,
        );
        Ok(Some(v_ptr))
    }

    /// rweakref.py:91-96
    fn weakref_deref(&self, hop: &HighLevelOp, v_wref: Hlvalue) -> RTypeResult {
        let c_ref = ref_field_const()?;
        let v_gcref = hop
            .genop(
                "getfield",
                vec![v_wref, Hlvalue::Constant(c_ref)],
                GenopResult::LLType(gcref_type()),
            )
            .ok_or_else(|| TyperError::message("EmulatedWeakRefRepr: getfield genop failed"))?;
        let r_result = hop.r_result.borrow();
        let r_result = r_result.as_ref().ok_or_else(|| {
            TyperError::message("EmulatedWeakRefRepr._weakref_deref: no r_result")
        })?;
        Ok(hop.genop(
            "cast_opaque_ptr",
            vec![v_gcref],
            GenopResult::Repr(r_result.clone()),
        ))
    }
}

impl fmt::Display for EmulatedWeakRefRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.repr_string())
    }
}

// ─── rtyper_makerepr dispatch ────────────────────────────────────────

/// rweakref.py:13-17 `SomeWeakRef.rtyper_makerepr`.
pub fn weakref_makerepr(rtyper: &RPythonTyper) -> Result<Arc<dyn Repr>, TyperError> {
    let rweakref = rtyper
        .getconfig()
        .map(|c| c.translation.rweakref)
        .unwrap_or(true);
    if rweakref {
        Ok(Arc::new(WeakRefRepr::new()) as Arc<dyn Repr>)
    } else {
        Ok(Arc::new(EmulatedWeakRefRepr::new()) as Arc<dyn Repr>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flowspace::model::HostObject;

    #[test]
    fn weakref_convert_const_none_is_null_weakref_ptr() {
        let repr = WeakRefRepr::new();
        let c = repr.convert_const(&ConstValue::None).unwrap();
        assert_eq!(c.concretetype, Some(WEAKREF_PTR.clone()));
        let ConstValue::LLPtr(ptr) = c.value else {
            panic!("None weakref constant must be a low-level null pointer");
        };
        assert!(!ptr.nonzero());
    }

    #[test]
    fn weakref_convert_const_dead_host_weakref_is_dead_wref_ptr() {
        let repr = WeakRefRepr::new();
        let dead = HostObject::new_weakref("weakref.ref", None);
        let c = repr
            .convert_const(&ConstValue::HostObject(dead))
            .expect("dead host weakref must convert to llmemory.dead_wref shape");
        assert_eq!(c.concretetype, Some(WEAKREF_PTR.clone()));
        let ConstValue::LLPtr(ptr) = c.value else {
            panic!("dead weakref constant must be a low-level weakref pointer");
        };
        assert!(ptr.nonzero());
    }

    #[test]
    fn emulated_weakref_convert_const_none_is_null_emulated_ptr() {
        let repr = EmulatedWeakRefRepr::new();
        let c = repr.convert_const(&ConstValue::None).unwrap();
        assert_eq!(c.concretetype, Some(repr.lltype.clone()));
        let ConstValue::LLPtr(ptr) = c.value else {
            panic!("None emulated weakref constant must be a low-level null pointer");
        };
        assert!(!ptr.nonzero());
    }
}
