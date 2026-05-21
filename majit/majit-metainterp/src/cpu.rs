//! Backend CPU abstraction per `rpython/jit/backend/model.py`.
//!
//! RPython's `AbstractCPU` (model.py:39+) hosts the services every
//! `Optimization` sub-class reaches via `self.optimizer.cpu.<method>()`:
//! `cls_of_box(box)` (model.py:199-201), `bh_*` runtime calls
//! (model.py:209+), GC type-info accessors, and so on.  Pyre currently
//! exposes only `cls_of_box` here; future expansion ports the rest of
//! the AbstractCPU surface onto the same trait so the carrier chain
//! `MetaInterp.cpu → UnrollOpt.cpu → Optimizer.cpu → OptContext.cpu`
//! threads a single trait object instead of an N-tuple of `fn` pointers.

use std::sync::Arc;

use crate::r#box::BoxRef;
use majit_ir::{FieldDescr, GcRef, Value};

/// `model.py:39 AbstractCPU` (subset) — services hosted on
/// `optimizer.cpu` and reached from any `Optimization` sub-class.
pub trait Cpu: Send + Sync {
    /// `model.py:199-201 cpu.cls_of_box(box)`:
    ///
    /// ```python
    /// def cls_of_box(self, box):
    ///     obj = lltype.cast_opaque_ptr(OBJECTPTR, box.getref_base())
    ///     return ConstInt(ptr2int(obj.typeptr))
    /// ```
    ///
    /// Reads the runtime typeptr (object class) at offset 0 of the
    /// box's Ref payload — the lltype `OBJECTPTR` layout that the
    /// default backend uses.  Returns 0 when the box does not carry a
    /// concrete `Value::Ref` or when the Ref is null.  Backends that
    /// enable `gcremovetypeptr` route through `model.py:266+` and
    /// override this method to consult the GC header instead.
    fn cls_of_box(&self, box_: &BoxRef) -> i64;

    /// `model.py:209+ cpu.bh_getfield_gc_i / _r / _f`:
    /// `llmodel.py:467-478 read_int_at_mem / read_ref_at_mem / read_float_at_mem`.
    /// Read the field at `struct_ptr + fielddescr.offset()` honoring
    /// `field_size` + `is_field_signed`. The pure-getfield constant
    /// folder (`executor::execute_nonspec_const`) calls these after
    /// `protect_speculative_field` has validated that `struct_ptr` is
    /// non-null and of the expected type.
    fn bh_getfield_gc_i(&self, struct_ptr: usize, fielddescr: &dyn FieldDescr) -> i64;
    fn bh_getfield_gc_r(&self, struct_ptr: usize, fielddescr: &dyn FieldDescr) -> GcRef;
    fn bh_getfield_gc_f(&self, struct_ptr: usize, fielddescr: &dyn FieldDescr) -> f64;
}

/// Default `Cpu` implementing `cls_of_box` against the lltype-typeptr-
/// at-offset-0 layout (model.py:199-201).  Production paths that did
/// not install a custom backend hook fall through to this.
pub struct DefaultCpu;

impl Cpu for DefaultCpu {
    fn cls_of_box(&self, box_: &BoxRef) -> i64 {
        // resoperation.py:57-68 walker to the terminal Const.
        let raw = match box_.get_box_replacement(false).const_value() {
            Some(Value::Ref(gcref)) if !gcref.is_null() => gcref.0 as i64,
            _ => return 0,
        };
        // SAFETY: caller has guaranteed `raw` is a valid OBJECTPTR payload
        // pointer; the lltype OBJECTPTR layout has the typeptr at offset 0
        // (model.py:200 `box.getref_base().typeptr`).
        unsafe { *(raw as *const usize) as i64 }
    }

    fn bh_getfield_gc_i(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> i64 {
        // llmodel.py:467-478 read_int_at_mem signed/unsigned width
        // dispatch. RPython's loop falls through to `else: raise
        // NotImplementedError("size = %d" % size)` when no `itemsize`
        // matches; mirror that with a panic. Callers that may receive
        // exotic field sizes (e.g. the trace-time fold path) MUST
        // pre-filter via `fd.field_size()` before invoking this method.
        let addr = struct_ptr + fd.offset();
        match (fd.field_size(), fd.is_field_signed()) {
            (8, true) => unsafe { *(addr as *const i64) },
            (8, false) => unsafe { *(addr as *const u64) as i64 },
            (4, true) => unsafe { *(addr as *const i32) as i64 },
            (4, false) => unsafe { *(addr as *const u32) as i64 },
            (2, true) => unsafe { *(addr as *const i16) as i64 },
            (2, false) => unsafe { *(addr as *const u16) as i64 },
            (1, true) => unsafe { *(addr as *const i8) as i64 },
            (1, false) => unsafe { *(addr as *const u8) as i64 },
            (size, _) => panic!(
                "bh_getfield_gc_i: unsupported field size {} \
                 (llmodel.py:478 NotImplementedError)",
                size
            ),
        }
    }

    fn bh_getfield_gc_r(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> GcRef {
        // llmodel.py read_ref_at_mem — pointer width.
        let addr = struct_ptr + fd.offset();
        GcRef(unsafe { *(addr as *const usize) })
    }

    fn bh_getfield_gc_f(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> f64 {
        // llmodel.py read_float_at_mem — 64-bit IEEE.
        let addr = struct_ptr + fd.offset();
        let bits = unsafe { *(addr as *const u64) };
        f64::from_bits(bits)
    }
}

/// `Arc<dyn Cpu>` factory for callers that previously installed a bare
/// `fn(i64) -> i64` hook.  Wraps the fn pointer in a struct that
/// extracts the raw Ref value from the BoxRef before invoking the
/// closure, so existing `set_cls_of_box(fn)` call sites continue to
/// receive the raw runtime payload.
pub fn cpu_from_cls_of_box_fn(f: fn(i64) -> i64) -> Arc<dyn Cpu> {
    struct ClosureCpu(fn(i64) -> i64);
    impl Cpu for ClosureCpu {
        fn cls_of_box(&self, box_: &BoxRef) -> i64 {
            let raw = match box_.get_box_replacement(false).const_value() {
                Some(Value::Ref(gcref)) => gcref.0 as i64,
                _ => 0,
            };
            (self.0)(raw)
        }
        fn bh_getfield_gc_i(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> i64 {
            DefaultCpu.bh_getfield_gc_i(struct_ptr, fd)
        }
        fn bh_getfield_gc_r(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> GcRef {
            DefaultCpu.bh_getfield_gc_r(struct_ptr, fd)
        }
        fn bh_getfield_gc_f(&self, struct_ptr: usize, fd: &dyn FieldDescr) -> f64 {
            DefaultCpu.bh_getfield_gc_f(struct_ptr, fd)
        }
    }
    Arc::new(ClosureCpu(f))
}

/// `Arc<dyn Cpu>` to the default lltype backend, for production paths
/// + tests that want the model.py:199-201 typeptr-at-offset-0 read.
pub fn default_cpu() -> Arc<dyn Cpu> {
    Arc::new(DefaultCpu)
}
