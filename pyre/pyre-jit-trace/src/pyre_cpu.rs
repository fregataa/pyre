//! `Cpu` trait impl for pyre's runtime string layout.
//!
//! `llmodel.py:557 gc_ll_descr.str_descr / unicode_descr` parity — the
//! typed `ArrayDescr` that backend init caches and that the speculative
//! protect / length read / per-character read all route through.
//! `model.py:209+` for the read-family. PyPy stores
//! `gc_ll_descr.str_descr` once at backend init; pyre exposes it via
//! the `Cpu` trait so `protect_speculative_string`, `bh_strlen` and
//! `bh_strgetitem` all reach the same descr.
//!
//! Python 3 unifies `str` and `unicode` into one `W_StrObject`
//! (UTF-8), so `str_descr()` and `unicode_descr()` return the same
//! `PyreStrDescr`.
//!
//! `W_StrObject` (pyre-object) stores char data behind a
//! `*mut String` pointer at `STR_VALUE_OFFSET`; the default
//! `bh_getarrayitem_gc_i(base + index)` path would read wrong memory,
//! so `bh_strgetitem` is overridden to follow the indirection.

use std::sync::{Arc, OnceLock};

use majit_ir::{ArrayDescr, Descr, FieldDescr, GcRef, Type};
use majit_metainterp::r#box::BoxRef;
use majit_metainterp::cpu::{Cpu, DefaultCpu};
use pyre_object::strobject::{
    STR_LEN_OFFSET, STR_VALUE_OFFSET, W_STR_GC_TYPE_ID, W_STR_OBJECT_SIZE,
};

/// `descr.py FieldDescr` for `W_StrObject.len` — the cached length
/// field at offset `STR_LEN_OFFSET`, 8-byte `usize`.  Consulted by
/// `bh_arraylen_gc` per `llmodel.py:594-595`.
#[derive(Debug)]
struct PyreStrLenFieldDescr;

impl Descr for PyreStrLenFieldDescr {}

impl FieldDescr for PyreStrLenFieldDescr {
    fn offset(&self) -> usize {
        STR_LEN_OFFSET
    }
    fn field_size(&self) -> usize {
        8
    }
    fn field_type(&self) -> Type {
        Type::Int
    }
    fn is_field_signed(&self) -> bool {
        // `W_StrObject.len: usize` — `bh_arraylen_gc` reads via
        // `*(addr as *const i64)` directly, but the upstream
        // `read_int_at_mem(..., WORD, 1)` at `llmodel.py:587` is
        // signed.  Keep signed=true to mirror that.
        true
    }
    fn field_name(&self) -> &'static str {
        "W_StrObject.len"
    }
}

/// `descr.py ArrayDescr` for `W_StrObject` (Python 3 `str`, UTF-8
/// bytes).  `base_size` is `W_STR_OBJECT_SIZE` (full struct header
/// before items would start in the in-line layout that PyPy's STR
/// uses); `item_size` is 1 byte; `type_id` matches the GC tid the
/// allocator stamps onto every `W_StrObject`.
#[derive(Debug)]
struct PyreStrDescr;

const PYRE_STR_LEN_FIELD_DESCR: PyreStrLenFieldDescr = PyreStrLenFieldDescr;
const PYRE_STR_DESCR: PyreStrDescr = PyreStrDescr;

impl Descr for PyreStrDescr {}

impl ArrayDescr for PyreStrDescr {
    fn base_size(&self) -> usize {
        W_STR_OBJECT_SIZE
    }
    fn item_size(&self) -> usize {
        1
    }
    fn type_id(&self) -> u32 {
        W_STR_GC_TYPE_ID as u32
    }
    fn item_type(&self) -> Type {
        Type::Int
    }
    fn is_item_signed(&self) -> bool {
        false
    }
    fn len_descr(&self) -> Option<&dyn FieldDescr> {
        Some(&PYRE_STR_LEN_FIELD_DESCR)
    }
}

/// `Cpu` impl for pyre's runtime.  Delegates to `DefaultCpu` for the
/// methods `DefaultCpu` overrides (`cls_of_box` / `cls_of_gcref` /
/// `bh_getfield_gc_{i,r,f}`) and exposes pyre-specific descrs for the
/// str / unicode family.  `bh_strgetitem` / `bh_unicodegetitem` follow
/// the `W_StrObject.value: *mut String` indirection that PyPy's STR
/// layout does not need (PyPy stores chars in-line after the header).
pub struct PyreCpu(DefaultCpu);

impl PyreCpu {
    pub fn new() -> Self {
        Self(DefaultCpu)
    }
}

impl Default for PyreCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu for PyreCpu {
    fn cls_of_box(&self, box_: &BoxRef) -> i64 {
        self.0.cls_of_box(box_)
    }
    fn cls_of_gcref(&self, gcref: GcRef) -> i64 {
        self.0.cls_of_gcref(gcref)
    }
    fn bh_getfield_gc_i(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> i64 {
        self.0.bh_getfield_gc_i(struct_ptr, fd)
    }
    fn bh_getfield_gc_r(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> GcRef {
        self.0.bh_getfield_gc_r(struct_ptr, fd)
    }
    fn bh_getfield_gc_f(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> f64 {
        self.0.bh_getfield_gc_f(struct_ptr, fd)
    }

    fn str_descr(&self) -> Option<&dyn ArrayDescr> {
        Some(&PYRE_STR_DESCR)
    }
    fn unicode_descr(&self) -> Option<&dyn ArrayDescr> {
        Some(&PYRE_STR_DESCR)
    }

    fn bh_strlen(&self, string: GcRef) -> Option<i64> {
        // RPython STR is `Array(Char)` byte string (`rstr.py:1226-1228`);
        // `llmodel.py:667 bh_strlen` returns the byte count.  The default
        // impl reads `W_StrObject.len` which is `s.chars().count()`
        // (codepoint count, `strobject.rs:51`) — wrong for STR semantics.
        // Follow the `*mut String` indirection and return `s.len()` (byte
        // count), so "é" (UTF-8 = `[195, 169]`) yields 2, not 1.
        if string.is_null() {
            return None;
        }
        let value_addr = string.0 + STR_VALUE_OFFSET;
        let value_ptr = unsafe { *(value_addr as *const *const String) };
        if value_ptr.is_null() {
            return None;
        }
        let s = unsafe { &*value_ptr };
        Some(s.len() as i64)
    }

    fn bh_strgetitem(&self, string: GcRef, index: i64) -> Option<i64> {
        // RPython STR is `Array(Char)` byte string (`rstr.py:1226-1228`);
        // STRGETITEM returns `ord(char)` = byte value.
        // `intbounds.rs:3109` narrows the result to `[0, 255]`
        // (`vstring.py:393-400 IntBound.make_ge(0).make_lt(256)`).
        // `W_StrObject.value: *mut String` at `STR_VALUE_OFFSET` —
        // follow the indirection and read the UTF-8 byte at `index`.
        // PyPy's STR stores chars in-line at `base + item_size * index`;
        // pyre diverges structurally so this override replaces the
        // default `bh_getarrayitem_gc_i` routing.
        if string.is_null() {
            return None;
        }
        let value_addr = string.0 + STR_VALUE_OFFSET;
        let value_ptr = unsafe { *(value_addr as *const *const String) };
        if value_ptr.is_null() {
            return None;
        }
        let s = unsafe { &*value_ptr };
        let bytes = s.as_bytes();
        let i = index as usize;
        if i >= bytes.len() {
            return None;
        }
        Some(bytes[i] as i64)
    }

    fn bh_unicodegetitem(&self, unicode: GcRef, index: i64) -> Option<i64> {
        // RPython UNICODE is codepoint-indexed; UNICODEGETITEM returns
        // the codepoint value.  Pyre's `W_StrObject` stores UTF-8, so
        // walk codepoints via `chars().nth(index)`.
        if unicode.is_null() {
            return None;
        }
        let value_addr = unicode.0 + STR_VALUE_OFFSET;
        let value_ptr = unsafe { *(value_addr as *const *const String) };
        if value_ptr.is_null() {
            return None;
        }
        let s = unsafe { &*value_ptr };
        let i = index as usize;
        s.chars().nth(i).map(|c| c as i64)
    }
}

/// Shared `Arc<dyn Cpu>` for pyre.  Initialised once per process and
/// installed on `MetaInterp<PyreMeta>` via `set_cpu` at the
/// `trace_bytecode` entry point.
pub fn shared() -> Arc<dyn Cpu> {
    static CELL: OnceLock<Arc<dyn Cpu>> = OnceLock::new();
    CELL.get_or_init(|| Arc::new(PyreCpu::new()) as Arc<dyn Cpu>)
        .clone()
}
