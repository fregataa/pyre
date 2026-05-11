//! `pypy/interpreter/typedef.py:312-345 GetSetProperty` parity port.
//!
//! PyPy stores `fget` / `fset` / `fdel` / `doc` / `reqcls` /
//! `use_closure` / `name` as instance fields on the GetSetProperty
//! object itself — `class GetSetProperty(W_Root): _immutable_fields_
//! = [...]` (typedef.py:312-326).  Pyre previously emulated this with
//! a process-global `RwLock<HashMap<usize, GetSetFields>>` keyed by
//! descriptor pointer; that side table was a pure adaptation with no
//! RPython justification (and quietly leaked entries when descriptors
//! were collected).
//!
//! This module replaces the side table with a real W_Root struct
//! whose layout mirrors PyPy's instance shape line-for-line — readers
//! reach the slots via `&*(obj as *const W_GetSetProperty)`, the GC
//! traces every `PyObjectRef`-shaped field, and there is no global
//! state to fall out of sync with the descriptor's actual lifetime.

use crate::pyobject::*;

/// `pypy/interpreter/typedef.py:444 GetSetProperty.typedef = TypeDef(
/// "getset_descriptor", ...)`.  Pyre needs a static PyType here so the
/// W_GetSetProperty allocation can name an `ob_type` that's distinct
/// from `INSTANCE_TYPE` — the GC then routes `[malloc_typed,
/// register_vtable_for_type]` through this PyType pointer to find the
/// matching tid + offsets.
pub static GETSET_DESCRIPTOR_TYPE: PyType = crate::pyobject::new_pytype("getset_descriptor");

/// `pypy/interpreter/typedef.py:312-346 class GetSetProperty(W_Root)`.
///
/// All `PyObjectRef`-shaped slots default to `PY_NULL` to mark
/// "absent" (PyPy uses `None`); `use_closure` is a `bool` mirroring
/// the eponymous PyPy field.
#[repr(C)]
pub struct W_GetSetProperty {
    pub ob_header: PyObject,
    /// `typedef.py:339 self.fget` — getter callable.
    pub fget: PyObjectRef,
    /// `typedef.py:340 self.fset` — setter callable.
    pub fset: PyObjectRef,
    /// `typedef.py:341 self.fdel` — deleter callable.
    pub fdel: PyObjectRef,
    /// `typedef.py:342 self.doc` — wrapped docstring.
    pub doc: PyObjectRef,
    /// `typedef.py:343 self.reqcls` — required receiver class for
    /// `descr_self_interp_w` mismatch checking.
    pub reqcls: PyObjectRef,
    /// `typedef.py:346 self.name` — descriptor name (defaults to
    /// `'<generic property>'` when the caller passes None).
    pub name: PyObjectRef,
    /// `typedef.py:320 w_objclass = None` class default + per-instance
    /// override stamped by `copy_for_type` (typedef.py:353).  Read by
    /// `descr_get_objclass` (typedef.py:414-418) before falling back
    /// to `space.gettypeobject(self.reqcls.typedef)`.
    pub w_objclass: PyObjectRef,
    /// `typedef.py:344 self.w_qualname = None` — lazy cache for
    /// `descr_get_qualname` (typedef.py:420-433); first reader stamps
    /// `"<class>.<name>"` (or `"?.<name>"` when `reqcls is None`).
    pub w_qualname: PyObjectRef,
    /// `typedef.py:345 self.use_closure` — passes `(self, space, obj)`
    /// vs `(space, obj)` to the wrapped callbacks.
    pub use_closure: bool,
}

pub const GETSET_FGET_OFFSET: usize = std::mem::offset_of!(W_GetSetProperty, fget);
pub const GETSET_FSET_OFFSET: usize = std::mem::offset_of!(W_GetSetProperty, fset);
pub const GETSET_FDEL_OFFSET: usize = std::mem::offset_of!(W_GetSetProperty, fdel);
pub const GETSET_DOC_OFFSET: usize = std::mem::offset_of!(W_GetSetProperty, doc);
pub const GETSET_REQCLS_OFFSET: usize = std::mem::offset_of!(W_GetSetProperty, reqcls);
pub const GETSET_NAME_OFFSET: usize = std::mem::offset_of!(W_GetSetProperty, name);
pub const GETSET_W_OBJCLASS_OFFSET: usize = std::mem::offset_of!(W_GetSetProperty, w_objclass);
pub const GETSET_W_QUALNAME_OFFSET: usize = std::mem::offset_of!(W_GetSetProperty, w_qualname);

/// GC type id assigned to `W_GetSetProperty`.
/// 1-39 are taken; 40 is the next free slot.
pub const W_GETSET_PROPERTY_GC_TYPE_ID: u32 = 40;

pub const W_GETSET_PROPERTY_OBJECT_SIZE: usize = std::mem::size_of::<W_GetSetProperty>();

/// Eight PyObjectRef-shaped fields are GC roots — a GetSetProperty
/// can reference both interp-level callables (built-in functions) and
/// user-level ones, plus an optional `reqcls` / `w_objclass`
/// W_TypeObject and a lazily-cached `w_qualname` text object.
pub const W_GETSET_PROPERTY_GC_PTR_OFFSETS: [usize; 8] = [
    GETSET_FGET_OFFSET,
    GETSET_FSET_OFFSET,
    GETSET_FDEL_OFFSET,
    GETSET_DOC_OFFSET,
    GETSET_REQCLS_OFFSET,
    GETSET_NAME_OFFSET,
    GETSET_W_OBJCLASS_OFFSET,
    GETSET_W_QUALNAME_OFFSET,
];

impl crate::lltype::GcType for W_GetSetProperty {
    const TYPE_ID: u32 = W_GETSET_PROPERTY_GC_TYPE_ID;
    const SIZE: usize = W_GETSET_PROPERTY_OBJECT_SIZE;
}

/// Allocate a `W_GetSetProperty` bound to `GETSET_DESCRIPTOR_TYPE`.
/// Mirrors `typedef.py:327-336 _init` — every slot is set in one shot
/// so the descriptor is fully initialised before the first reader.
///
/// `name` may be `PY_NULL`, in which case the caller is responsible
/// for substituting `'<generic property>'` (matching `typedef.py:336
/// self.name = name if name is not None else '<generic property>'`);
/// pyre's call sites pass an already-resolved name to keep the
/// allocation hot path branchless.
pub fn w_getset_property_new(
    fget: PyObjectRef,
    fset: PyObjectRef,
    fdel: PyObjectRef,
    doc: PyObjectRef,
    reqcls: PyObjectRef,
    use_closure: bool,
    name: PyObjectRef,
) -> PyObjectRef {
    crate::lltype::malloc_typed(W_GetSetProperty {
        ob_header: PyObject {
            ob_type: &GETSET_DESCRIPTOR_TYPE as *const PyType,
            w_class: get_instantiate(&GETSET_DESCRIPTOR_TYPE),
        },
        fget,
        fset,
        fdel,
        doc,
        reqcls,
        name,
        w_objclass: PY_NULL,
        w_qualname: PY_NULL,
        use_closure,
    }) as PyObjectRef
}

/// Test whether `obj` is a `W_GetSetProperty`.
///
/// # Safety
/// `obj` must be a valid, non-null pointer to a `PyObject`.
#[inline]
pub unsafe fn is_getset_property(obj: PyObjectRef) -> bool {
    unsafe { py_type_check(obj, &GETSET_DESCRIPTOR_TYPE) }
}

/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_fget(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_GetSetProperty)).fget }
}

/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_fset(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_GetSetProperty)).fset }
}

/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_fdel(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_GetSetProperty)).fdel }
}

/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_reqcls(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_GetSetProperty)).reqcls }
}

/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_name(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_GetSetProperty)).name }
}

/// `typedef.py:58 add_entries` parity — overwrite the descriptor's
/// `name` slot with the dict-key it was registered under.  Used by
/// the post-init namespace walker so descriptors built without an
/// explicit name (most `make_getset_descriptor` callers) carry the
/// matching `__name__` instead of the `<generic property>` sentinel.
///
/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_set_name(obj: PyObjectRef, value: PyObjectRef) {
    unsafe { (*(obj as *mut W_GetSetProperty)).name = value }
}

/// `typedef.py:343 self.reqcls = cls` — write the required-receiver
/// class slot.  Used by `patch_builtin_function_descriptors` to
/// install the BuiltinFunction class onto the shared
/// `__self__`/`__doc__` GetSetProperty descriptors after the
/// W_TypeObject for BuiltinFunction is materialised.
#[inline]
pub unsafe fn w_getset_set_reqcls(obj: PyObjectRef, value: PyObjectRef) {
    unsafe { (*(obj as *mut W_GetSetProperty)).reqcls = value }
}

/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_doc(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_GetSetProperty)).doc }
}

/// `typedef.py:320 / 348-356 copy_for_type` writes `new.w_objclass`.
/// Pyre keeps the slot directly on the struct so the descriptor's
/// `descr_get_objclass` reads it without any side-table.
///
/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_objclass(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_GetSetProperty)).w_objclass }
}

/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_set_objclass(obj: PyObjectRef, value: PyObjectRef) {
    unsafe { (*(obj as *mut W_GetSetProperty)).w_objclass = value }
}

/// `typedef.py:344 self.w_qualname = None` — lazy cache slot for
/// `descr_get_qualname` (typedef.py:420-433).
///
/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_qualname(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_GetSetProperty)).w_qualname }
}

/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_set_qualname(obj: PyObjectRef, value: PyObjectRef) {
    unsafe { (*(obj as *mut W_GetSetProperty)).w_qualname = value }
}

/// `typedef.py:345 self.use_closure` — read-only accessor.
///
/// # Safety
/// `obj` must point to a valid `W_GetSetProperty`.
#[inline]
pub unsafe fn w_getset_get_use_closure(obj: PyObjectRef) -> bool {
    unsafe { (*(obj as *const W_GetSetProperty)).use_closure }
}
