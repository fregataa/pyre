//! `pypy/module/_sre/interp_sre.py:147 W_SRE_Pattern` /
//! `:675 W_SRE_Match` — typed layouts for compiled patterns and match
//! results.  Engine state lives in interp-level fields, not in a user
//! attribute store.

use crate::pyobject::*;
use pyre_macros::pyre_class;

/// Compiled regular expression object (interp_sre.py:147).
///
/// `code`/`code_len` stand in for `srepat.code =
/// rsre_core.CompiledPattern(code, flags)` (interp_sre.py:635): pyre
/// runs the sre-engine crate's u32 opcode buffer, leaked once at
/// compile time and immutable for the pattern's lifetime
/// (`_immutable_fields_ = ["code", ...]`, interp_sre.py:148).
#[pyre_class("re.Pattern", static_name = "SRE_PATTERN")]
pub struct W_SRE_Pattern {
    /// interp_sre.py:630 `srepat.w_pattern` — original uncompiled pattern.
    pub w_pattern: PyObjectRef,
    /// interp_sre.py:631 `srepat.flags`.
    pub flags: i64,
    /// interp_sre.py:635 `srepat.code` (see type doc).
    pub code: *const u32,
    pub code_len: usize,
    /// interp_sre.py:637 `srepat.num_groups`.
    pub num_groups: i64,
    /// interp_sre.py:638 `srepat.w_groupindex`.
    pub w_groupindex: PyObjectRef,
    /// interp_sre.py:639 `srepat.w_indexgroup`.
    pub w_indexgroup: PyObjectRef,
}

/// Allocate a `W_SRE_Pattern` — `SRE_Pattern__new__` field stamping
/// (interp_sre.py:624-639).
pub fn w_sre_pattern_new(
    w_pattern: PyObjectRef,
    flags: i64,
    code: &'static [u32],
    num_groups: i64,
    w_groupindex: PyObjectRef,
    w_indexgroup: PyObjectRef,
) -> PyObjectRef {
    // `gct_fv_gc_malloc` bracket pattern (`framework.py:853-856`).
    let _roots = crate::gc_roots::push_roots();
    crate::gc_roots::pin_root(w_pattern);
    crate::gc_roots::pin_root(w_groupindex);
    crate::gc_roots::pin_root(w_indexgroup);
    W_SRE_Pattern::allocate(W_SRE_Pattern {
        ob: PyObject {
            ob_type: std::ptr::null(),
            w_class: std::ptr::null_mut(),
        },
        w_pattern,
        flags,
        code: code.as_ptr(),
        code_len: code.len(),
        num_groups,
        w_groupindex,
        w_indexgroup,
    })
}

/// # Safety
/// `obj` must be a valid, non-null pointer to a `PyObject`.
#[inline]
pub unsafe fn is_sre_pattern(obj: PyObjectRef) -> bool {
    unsafe { py_type_check(obj, &SRE_PATTERN_TYPE) }
}

/// Match result object (interp_sre.py:675).
///
/// Upstream keeps the live match context (`self.ctx`) and flattens the
/// group marks lazily (`flatten_marks`, interp_sre.py:793-797).  Pyre's
/// sre-engine surfaces the marks eagerly at match time, so the span
/// table (group 0 = whole match first, `(-1, -1)` = unmatched group)
/// is materialised once into a leaked buffer that plays both `ctx`
/// and `flatten_cache`.
#[pyre_class("re.Match", static_name = "SRE_MATCH")]
pub struct W_SRE_Match {
    /// interp_sre.py:680 `self.srepat`.
    pub w_srepat: PyObjectRef,
    /// interp_sre.py:682 `self.w_string`.
    pub w_string: PyObjectRef,
    /// `ctx.original_pos` (fget_pos, interp_sre.py:851-852).
    pub pos: i64,
    /// `ctx.end` (fget_endpos, interp_sre.py:854-855).
    pub endpos: i64,
    /// `_last_index()` (interp_sre.py:825-829); `-1` plays None.
    pub lastindex: i64,
    /// Flattened spans (see type doc).
    pub spans: *const (i64, i64),
    pub spans_len: usize,
}

/// Allocate a `W_SRE_Match` — `W_SRE_Match.__init__` field stamping
/// (interp_sre.py:678-682) plus the eager span flattening described on
/// the type.
pub fn w_sre_match_new(
    w_srepat: PyObjectRef,
    w_string: PyObjectRef,
    pos: i64,
    endpos: i64,
    lastindex: i64,
    spans: &'static [(i64, i64)],
) -> PyObjectRef {
    // `gct_fv_gc_malloc` bracket pattern (`framework.py:853-856`).
    let _roots = crate::gc_roots::push_roots();
    crate::gc_roots::pin_root(w_srepat);
    crate::gc_roots::pin_root(w_string);
    W_SRE_Match::allocate(W_SRE_Match {
        ob: PyObject {
            ob_type: std::ptr::null(),
            w_class: std::ptr::null_mut(),
        },
        w_srepat,
        w_string,
        pos,
        endpos,
        lastindex,
        spans: spans.as_ptr(),
        spans_len: spans.len(),
    })
}

/// # Safety
/// `obj` must be a valid, non-null pointer to a `PyObject`.
#[inline]
pub unsafe fn is_sre_match(obj: PyObjectRef) -> bool {
    unsafe { py_type_check(obj, &SRE_MATCH_TYPE) }
}

/// The flattened span for group `groupnum` (0 = whole match), or
/// `None` past the table — `do_span`'s table read (interp_sre.py:817-820).
///
/// # Safety
/// `obj` must point to a valid `W_SRE_Match`.
#[inline]
pub unsafe fn w_sre_match_get_span(obj: PyObjectRef, groupnum: usize) -> Option<(i64, i64)> {
    let m = obj as *const W_SRE_Match;
    unsafe {
        if groupnum >= (*m).spans_len {
            return None;
        }
        Some(*(*m).spans.add(groupnum))
    }
}

/// `_sre.SRE_Scanner` (interp_sre.py:904) — the stateful iterator behind
/// `Pattern.finditer` (and the undocumented `scanner()`), yielding a
/// `W_SRE_Match` per non-overlapping match.
///
/// Upstream keeps the live `rsre_core` context (`self.ctx`); pyre's
/// sre-engine context borrows the subject string and code, so it cannot
/// be parked in a GC object.  Instead the resumable cursor is reduced to
/// the character position `pos` and the `must_advance` flag — exactly
/// the two fields `SearchIter` threads across calls (engine.rs:255-256)
/// — and a fresh `Request`/`State` is rebuilt from the pattern + subject
/// on each step (both are leaked `&'static`, so this is stable across
/// callbacks).
/// `pos == -1` plays upstream's `self.ctx is None` exhausted state.
#[pyre_class("_sre.SRE_Scanner", static_name = "SRE_SCANNER")]
pub struct W_SRE_Scanner {
    /// interp_sre.py:907 `self.srepat`.
    pub w_srepat: PyObjectRef,
    /// interp_sre.py:910 `self.w_string`.
    pub w_string: PyObjectRef,
    /// Original search position (`ctx.original_pos`) exposed by each match.
    pub original_pos: i64,
    /// Character position of the next search (`ctx.match_start`); `-1` once
    /// the iterator is exhausted (`self.ctx is None`).
    pub pos: i64,
    /// Character end position (`ctx.end`) — the `endpos` argument of finditer.
    pub endpos: i64,
    /// `req.must_advance` (engine.rs:255) — set after a zero-width match so
    /// the next search refuses to re-match at the same position.
    pub must_advance: i64,
}

/// Allocate a `W_SRE_Scanner` — `W_SRE_Scanner.__init__` (interp_sre.py:905).
pub fn w_sre_scanner_new(
    w_srepat: PyObjectRef,
    w_string: PyObjectRef,
    pos: i64,
    endpos: i64,
) -> PyObjectRef {
    let _roots = crate::gc_roots::push_roots();
    crate::gc_roots::pin_root(w_srepat);
    crate::gc_roots::pin_root(w_string);
    W_SRE_Scanner::allocate(W_SRE_Scanner {
        ob: PyObject {
            ob_type: std::ptr::null(),
            w_class: std::ptr::null_mut(),
        },
        w_srepat,
        w_string,
        original_pos: pos,
        pos,
        endpos,
        must_advance: 0,
    })
}

/// # Safety
/// `obj` must be a valid, non-null pointer to a `PyObject`.
#[inline]
pub unsafe fn is_sre_scanner(obj: PyObjectRef) -> bool {
    unsafe { py_type_check(obj, &SRE_SCANNER_TYPE) }
}
