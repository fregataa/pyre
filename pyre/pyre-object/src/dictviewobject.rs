//! `pypy/objspace/std/dictmultiobject.py:449-470 W_DictMultiViewKeysObject`
//! / `W_DictMultiViewValuesObject` / `W_DictMultiViewItemsObject`
//! parity port.
//!
//! PyPy keeps three sibling W_Root types — one per view kind — that
//! all share the same shape: a back-reference to the source
//! `W_DictMultiObject` plus the iteration discipline appropriate to
//! the kind.  Pyre fuses them into a single `W_DictView` carrying a
//! `DictViewKind` tag so the three Python-visible types can share
//! the GC-traced `w_dict` slot and accessors; type identity is
//! restored at the W_TypeObject layer through the kind tag (see
//! `dict_view_type_for_kind`).

use crate::pyobject::*;

pub static DICT_KEYS_TYPE: PyType = crate::pyobject::new_pytype("dict_keys");
pub static DICT_VALUES_TYPE: PyType = crate::pyobject::new_pytype("dict_values");
pub static DICT_ITEMS_TYPE: PyType = crate::pyobject::new_pytype("dict_items");

/// `dictmultiobject.py:449/459/469` — three sibling view classes.
/// Pyre folds them into one struct + tag because the body is
/// otherwise identical (only the iteration / repr shape differs).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictViewKind {
    Keys = 0,
    Values = 1,
    Items = 2,
}

/// Layout: `[ob_header | kind: DictViewKind | w_dict: PyObjectRef]`.
///
/// `w_dict` is the live `W_DictObject` the view is attached to; PyPy's
/// `W_DictMultiViewKeysObject.w_dict` (`dictmultiobject.py:451`) plays
/// the same role.  Mutations on the source dict are visible through
/// the view because every reader (iter / len / contains) goes through
/// `w_dict` rather than caching a snapshot.
#[repr(C)]
pub struct W_DictView {
    pub ob_header: PyObject,
    pub kind: DictViewKind,
    pub w_dict: PyObjectRef,
}

pub const DICT_VIEW_KIND_OFFSET: usize = std::mem::offset_of!(W_DictView, kind);
pub const DICT_VIEW_W_DICT_OFFSET: usize = std::mem::offset_of!(W_DictView, w_dict);

/// GC type id assigned to `W_DictView` at JitDriver init time.
/// 32 is taken by `W_GENERATOR_GC_TYPE_ID`; the next free slot is 39
/// (one past `W_DICT_PROXY_GC_TYPE_ID = 38`).
pub const W_DICT_VIEW_GC_TYPE_ID: u32 = 39;

pub const W_DICT_VIEW_OBJECT_SIZE: usize = std::mem::size_of::<W_DictView>();

/// Single inline `PyObjectRef`-shaped field — the back-pointer to the
/// source dict.
pub const W_DICT_VIEW_GC_PTR_OFFSETS: [usize; 1] = [DICT_VIEW_W_DICT_OFFSET];

impl crate::lltype::GcType for W_DictView {
    const TYPE_ID: u32 = W_DICT_VIEW_GC_TYPE_ID;
    const SIZE: usize = W_DICT_VIEW_OBJECT_SIZE;
}

/// Pick the Python-visible type for a given view kind.  Mirrors
/// PyPy's three distinct W_TypeObject identities so
/// `type(d.keys()) is dict_keys`, `type(d.values()) is dict_values`,
/// `type(d.items()) is dict_items` all hold.
pub fn dict_view_type_for_kind(kind: DictViewKind) -> &'static PyType {
    match kind {
        DictViewKind::Keys => &DICT_KEYS_TYPE,
        DictViewKind::Values => &DICT_VALUES_TYPE,
        DictViewKind::Items => &DICT_ITEMS_TYPE,
    }
}

/// Allocate a fresh dict view bound to `w_dict`.
pub fn w_dict_view_new(w_dict: PyObjectRef, kind: DictViewKind) -> PyObjectRef {
    let tp = dict_view_type_for_kind(kind);
    crate::lltype::malloc_typed(W_DictView {
        ob_header: PyObject {
            ob_type: tp as *const PyType,
            w_class: get_instantiate(tp),
        },
        kind,
        w_dict,
    }) as PyObjectRef
}

/// Test whether `obj` is any of the three view types.
///
/// # Safety
/// `obj` must be a valid, non-null pointer to a `PyObject`.
#[inline]
pub unsafe fn is_dict_view(obj: PyObjectRef) -> bool {
    unsafe {
        py_type_check(obj, &DICT_KEYS_TYPE)
            || py_type_check(obj, &DICT_VALUES_TYPE)
            || py_type_check(obj, &DICT_ITEMS_TYPE)
    }
}

/// # Safety
/// `obj` must point to a valid `W_DictView`.
#[inline]
pub unsafe fn w_dict_view_get_kind(obj: PyObjectRef) -> DictViewKind {
    unsafe { (*(obj as *const W_DictView)).kind }
}

/// # Safety
/// `obj` must point to a valid `W_DictView`.
#[inline]
pub unsafe fn w_dict_view_get_dict(obj: PyObjectRef) -> PyObjectRef {
    unsafe { (*(obj as *const W_DictView)).w_dict }
}
