//! Builtin type method implementations.
//!
//! PyPy equivalents:
//!   pypy/objspace/std/listobject.py  (list methods)
//!   pypy/objspace/std/unicodeobject.py  (str methods)
//!   pypy/objspace/std/dictobject.py  (dict methods)
//!   pypy/objspace/std/tupleobject.py  (tuple methods)
//!
//! Separated from space.rs to avoid bloating the hot-path compilation
//! unit. Method functions are registered into TypeDef at startup.

use pyre_object::*;

// ── List methods ─────────────────────────────────────────────────────
// All take self (list) as first arg.

pub fn list_method_append(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() == 2, "append() takes exactly one argument");
    unsafe { w_list_append(args[0], args[1]) };
    Ok(w_none())
}

pub fn list_method_extend(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() == 2);
    let list = args[0];
    let other = args[1];
    unsafe {
        if is_list(other) {
            let n = w_list_len(other);
            for i in 0..n {
                if let Some(item) = w_list_getitem(other, i as i64) {
                    w_list_append(list, item);
                }
            }
        } else if is_tuple(other) {
            let n = w_tuple_len(other);
            for i in 0..n {
                if let Some(item) = w_tuple_getitem(other, i as i64) {
                    w_list_append(list, item);
                }
            }
        }
    }
    Ok(w_none())
}

/// PyPy: listobject.py descr_insert — list.insert(index, item)
pub fn list_method_insert(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() == 3, "insert() takes exactly 2 arguments");
    let index = unsafe { w_int_get_value(args[1]) };
    unsafe { pyre_object::listobject::w_list_insert(args[0], index, args[2]) };
    Ok(w_none())
}

/// PyPy: listobject.py descr_pop — list.pop([index])
pub fn list_method_pop(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty(), "pop() requires self");
    let index = if args.len() > 1 {
        unsafe { w_int_get_value(args[1]) }
    } else {
        -1 // default: pop last
    };
    unsafe {
        Ok(pyre_object::listobject::w_list_pop(args[0], index)
            .unwrap_or_else(|| panic!("pop from empty list")))
    }
}

/// PyPy: listobject.py descr_clear — list.clear()
pub fn list_method_clear(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    unsafe { pyre_object::listobject::w_list_clear(args[0]) };
    Ok(w_none())
}

/// PyPy: listobject.py descr_copy — list.copy()
pub fn list_method_copy(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let list = args[0];
    unsafe {
        let n = w_list_len(list);
        let mut items = Vec::with_capacity(n);
        for i in 0..n {
            if let Some(item) = w_list_getitem(list, i as i64) {
                items.push(item);
            }
        }
        Ok(w_list_new(items))
    }
}

/// PyPy: listobject.py descr_reverse — list.reverse()
pub fn list_method_reverse(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    unsafe { pyre_object::listobject::w_list_reverse(args[0]) };
    Ok(w_none())
}

/// PyPy: listobject.py descr_sort — list.sort()
///
/// Simplified: only sorts int lists. Full sort requires comparison protocol.
pub fn list_method_sort(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let list = args[0];
    unsafe {
        let n = w_list_len(list);
        let mut items: Vec<PyObjectRef> = (0..n)
            .filter_map(|i| w_list_getitem(list, i as i64))
            .collect();
        // Sort by int value (PyPy uses timsort with key/cmp)
        items.sort_by(|a, b| {
            if is_int(*a) && is_int(*b) {
                w_int_get_value(*a).cmp(&w_int_get_value(*b))
            } else {
                std::cmp::Ordering::Equal
            }
        });
        pyre_object::listobject::w_list_clear(list);
        for item in items {
            w_list_append(list, item);
        }
    }
    Ok(w_none())
}

/// listobject.py:795 `descr_index` — list.index(value[, start[, stop]]).
pub fn list_method_index(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2, "index() takes at least 1 argument");
    let list = args[0];
    let value = args[1];
    // listobject.py:799 unwrap_spec defaults: w_start=0 / w_stop=sys.maxint.
    // listobject.py:803 unwrap_start_stop handles negative normalization,
    // __index__ coercion and TypeError for non-index arguments.
    let size = unsafe { pyre_object::w_list_len(list) } as i64;
    let w_start = if args.len() >= 3 {
        args[2]
    } else {
        w_int_new(0)
    };
    let w_stop = if args.len() >= 4 {
        args[3]
    } else {
        w_int_new(i64::MAX)
    };
    let (start, stop) = crate::sliceobject::unwrap_start_stop(size, w_start, w_stop)?;
    match crate::listobject::w_list_find_or_count(list, value, start, stop, false)? {
        crate::listobject::FindOrCountResult::Index(i) => Ok(w_int_new(i)),
        crate::listobject::FindOrCountResult::NotFound => Err(crate::PyError::new(
            crate::PyErrorKind::ValueError,
            // listobject.py:805 `oefmt(space.w_ValueError, "%R is not in list", w_value)`
            format!("{} is not in list", unsafe {
                crate::display::py_repr(value)
            }),
        )),
        crate::listobject::FindOrCountResult::Count(_) => {
            unreachable!("find_or_count with count=false never returns Count")
        }
    }
}

/// listobject.py:744 `descr_count` — list.count(value)
pub fn list_method_count(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2, "count() takes exactly 1 argument");
    let list = args[0];
    let value = args[1];
    match crate::listobject::w_list_find_or_count(list, value, 0, i64::MAX, true)? {
        crate::listobject::FindOrCountResult::Count(n) => Ok(w_int_new(n)),
        crate::listobject::FindOrCountResult::NotFound => Ok(w_int_new(0)),
        crate::listobject::FindOrCountResult::Index(_) => {
            unreachable!("find_or_count with count=true never returns Index")
        }
    }
}

/// listobject.py:782 `descr_remove` — list.remove(value).
pub fn list_method_remove(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2, "remove() takes exactly 1 argument");
    crate::listobject::w_list_remove(args[0], args[1])?;
    Ok(w_none())
}

// ── String methods ───────────────────────────────────────────────────

pub fn str_method_join(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() == 2);
    let sep = unsafe { w_str_get_value(args[0]) };
    let iterable = args[1];
    let mut parts = Vec::new();
    unsafe {
        if is_list(iterable) {
            let n = w_list_len(iterable);
            for i in 0..n {
                if let Some(item) = w_list_getitem(iterable, i as i64) {
                    if is_str(item) {
                        parts.push(w_str_get_value(item).to_string());
                    }
                }
            }
        } else if is_tuple(iterable) {
            let n = w_tuple_len(iterable);
            for i in 0..n {
                if let Some(item) = w_tuple_getitem(iterable, i as i64) {
                    if is_str(item) {
                        parts.push(w_str_get_value(item).to_string());
                    }
                }
            }
        }
    }
    Ok(w_str_new(&parts.join(sep)))
}

pub fn str_method_split(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let sep = parse_split_sep(args.get(1).copied().unwrap_or(pyre_object::PY_NULL))?;
    // `unicodeobject.py:972 @unwrap_spec(maxsplit=int) descr_split` —
    // `space.int_w(w_maxsplit)` routes through `__index__`, so any
    // int-like object (subclass, numpy int, etc.) is accepted.
    let maxsplit = parse_split_maxsplit(args.get(2).copied().unwrap_or(pyre_object::PY_NULL))?;
    let sep_view = sep.as_deref();
    let parts: Vec<PyObjectRef> = match sep_view {
        Some(sep) => {
            // `unicodeobject.py:1028 _split_with_separator` raises
            // ValueError on empty separator before the slow path.
            if sep.is_empty() {
                return Err(crate::PyError::value_error("empty separator"));
            }
            if maxsplit < 0 {
                s.split(sep).map(|p| w_str_new(p)).collect()
            } else {
                s.splitn((maxsplit as usize) + 1, sep)
                    .map(|p| w_str_new(p))
                    .collect()
            }
        }
        None => {
            if maxsplit < 0 {
                s.split_whitespace().map(|p| w_str_new(p)).collect()
            } else {
                let mut out: Vec<PyObjectRef> = Vec::new();
                let mut remaining = s;
                let max = maxsplit as usize;
                while out.len() < max {
                    let trimmed = remaining.trim_start();
                    if trimmed.is_empty() {
                        break;
                    }
                    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
                    out.push(w_str_new(&trimmed[..end]));
                    remaining = &trimmed[end..];
                }
                let tail = remaining.trim_start();
                if !tail.is_empty() {
                    out.push(w_str_new(tail));
                }
                out
            }
        }
    };
    Ok(w_list_new(parts))
}

/// `pypy/objspace/std/unicodeobject.py:992-994 W_UnicodeObject
/// .convert_arg_to_w_unicode` parity — `sep` must be `None` or a
/// `str`; anything else surfaces a TypeError at the same call site
/// where PyPy's `space.unicode_w` would.
fn parse_split_sep(value: PyObjectRef) -> Result<Option<String>, crate::PyError> {
    if value.is_null() || unsafe { is_none(value) } {
        return Ok(None);
    }
    if unsafe { is_str(value) } {
        return Ok(Some(unsafe { w_str_get_value(value) }.to_string()));
    }
    let tp_name = unsafe {
        match crate::typedef::r#type(value) {
            Some(tp) => pyre_object::w_type_get_name(tp).to_string(),
            None => "object".to_string(),
        }
    };
    Err(crate::PyError::type_error(format!(
        "must be str or None, not {tp_name}"
    )))
}

/// `unicodeobject.py:972 @unwrap_spec(maxsplit=int)` parity —
/// `space.int_w(w_maxsplit)` routes through `__index__`, so any
/// int-like object is accepted; missing maxsplit defaults to -1
/// (unlimited).
fn parse_split_maxsplit(value: PyObjectRef) -> Result<i64, crate::PyError> {
    if value.is_null() || unsafe { is_none(value) } {
        return Ok(-1);
    }
    crate::builtins::space_index_w(value)
}

/// `pypy/objspace/std/unicodeobject.py:993-1024 W_UnicodeObject
/// .descr_rsplit`.  Mirrors `split` semantics in reverse — when
/// `maxsplit` is positive, only the rightmost `maxsplit` separators
/// participate.  Argument validation follows the same
/// `@unwrap_spec(maxsplit=int)` + `convert_arg_to_w_unicode` shape
/// as `descr_split`.
pub fn str_method_rsplit(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let sep = parse_split_sep(args.get(1).copied().unwrap_or(pyre_object::PY_NULL))?;
    let maxsplit = parse_split_maxsplit(args.get(2).copied().unwrap_or(pyre_object::PY_NULL))?;
    let sep_view = sep.as_deref();
    let parts: Vec<PyObjectRef> = match sep_view {
        Some(sep) => {
            // `unicodeobject.py:1028 _split_with_separator` raises
            // ValueError on empty separator before the slow path —
            // mirrors the forward `split` rejection.
            if sep.is_empty() {
                return Err(crate::PyError::value_error("empty separator"));
            }
            let mut out: Vec<&str> = if maxsplit < 0 {
                s.rsplit(sep).collect()
            } else {
                s.rsplitn((maxsplit as usize) + 1, sep).collect()
            };
            out.reverse();
            out.into_iter().map(|p| w_str_new(p)).collect()
        }
        None => {
            // `s.rsplit(None, maxsplit)` collapses runs of whitespace
            // and walks from the right.
            let chars: Vec<char> = s.chars().collect();
            let mut tokens: Vec<String> = Vec::new();
            let mut i = chars.len();
            let max = if maxsplit < 0 {
                usize::MAX
            } else {
                maxsplit as usize
            };
            while i > 0 && tokens.len() < max {
                while i > 0 && chars[i - 1].is_whitespace() {
                    i -= 1;
                }
                if i == 0 {
                    break;
                }
                let end = i;
                while i > 0 && !chars[i - 1].is_whitespace() {
                    i -= 1;
                }
                tokens.push(chars[i..end].iter().collect());
            }
            tokens.reverse();
            // Remaining prefix becomes the leading element.
            let prefix: String = chars[..i].iter().collect();
            let prefix_trimmed = prefix.trim();
            if !prefix_trimmed.is_empty() {
                let mut out = vec![w_str_new(prefix_trimmed)];
                out.extend(tokens.into_iter().map(|t| w_str_new(&t)));
                out
            } else {
                tokens.into_iter().map(|t| w_str_new(&t)).collect()
            }
        }
    };
    Ok(w_list_new(parts))
}

/// `pypy/objspace/std/unicodeobject.py:767-770 W_UnicodeObject.descr_casefold`:
///
/// ```python
/// def descr_casefold(self, space):
///     value = self._value
///     return space.newutf8(unicode_casefold(value), -1)
/// ```
///
/// PyPy delegates to `rpython.rlib.runicode.unicode_casefold` which
/// applies the full Unicode CaseFolding mapping (ß → ss,
/// ﬁ → fi, dotless-i variants, etc.).  Pyre has no Unicode-data
/// dependency, so this routes through Rust's `to_lowercase` and
/// applies a small whitelist of multi-character casefolds for the
/// most user-visible mismatches; coverage gap acknowledged inline.
///
/// **PRE-EXISTING-ADAPTATION** — full Unicode `CaseFolding.txt` (1500+
/// code points with status-T and status-F mappings) needs to land in
/// `pyre-object` before this can mirror `runicode.unicode_casefold`
/// line-by-line.  The whitelist below covers the German ß, the Latin
/// ligatures (ﬁ/ﬂ/ﬃ/ﬄ/ﬅ/ﬆ), and Turkish dotless-i variants —
/// the cases that surface in stdlib tests like
/// `str.casefold() == str.casefold()` round-trips.  Convergence
/// target: import the CaseFolding table as a generated `static`
/// lookup keyed by `char`.
pub fn str_method_casefold(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_str_new(&casefold_basic(s)))
}

fn casefold_basic(s: &str) -> String {
    // Multi-char expansion casefolds the Unicode `Final_Sigma` /
    // `Lt`/`Lu` cases pyre's interpreter test corpus actually
    // touches.  Any code point outside this whitelist falls back
    // to Rust's `char::to_lowercase` which matches CPython for the
    // overwhelming majority of letters.
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'ß' => out.push_str("ss"),
            'ﬀ' => out.push_str("ff"),
            'ﬁ' => out.push_str("fi"),
            'ﬂ' => out.push_str("fl"),
            'ﬃ' => out.push_str("ffi"),
            'ﬄ' => out.push_str("ffl"),
            'ﬅ' => out.push_str("st"),
            'ﬆ' => out.push_str("st"),
            'İ' => out.push_str("i\u{0307}"),
            _ => {
                for lower in ch.to_lowercase() {
                    out.push(lower);
                }
            }
        }
    }
    out
}

/// `pypy/objspace/std/unicodeobject.py:429-430 W_UnicodeObject
/// .descr_format_map` parity:
///
/// ```python
/// def descr_format_map(self, space, w_mapping):
///     return newformat.format_method(space, self, None, w_mapping, True)
/// ```
///
/// PyPy passes the mapping straight through to `format_method`; each
/// `{name}` field is then resolved by `space.getitem(mapping, w_key)`
/// at format-time, so the mapping is consulted *lazily*.  This
/// matters for mappings with side-effecting `__getitem__`, a
/// `__missing__` hook, or no `keys()` — pre-materialising via
/// `keys()` (the previous implementation) breaks `defaultdict`,
/// custom `Mapping` subclasses, and any object that only implements
/// `__getitem__`.
pub fn str_method_format_map(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let fmt = args[0];
    let mapping = args[1];
    str_method_format_core(fmt, &[], None, Some(mapping))
}

/// `pypy/objspace/std/unicodeobject.py W_UnicodeObject._strip` —
/// `s.strip([chars])`.  When `chars` is missing or None, defaults to
/// ASCII whitespace (the `str::trim` set).  When provided, removes
/// any character contained in `chars` from each end (NOT a substring
/// match — `'aabaa'.strip('a') == 'b'`).
fn strip_chars(s: &str, chars: Option<&str>, left: bool, right: bool) -> String {
    let chars_set: Option<Vec<char>> = chars.map(|c| c.chars().collect());
    let mut current: &str = s;
    if left {
        current = match chars_set.as_ref() {
            Some(set) => current.trim_start_matches(|c: char| set.contains(&c)),
            None => current.trim_start(),
        };
    }
    if right {
        current = match chars_set.as_ref() {
            Some(set) => current.trim_end_matches(|c: char| set.contains(&c)),
            None => current.trim_end(),
        };
    }
    current.to_string()
}

/// `pypy/objspace/std/unicodeobject.py:1464-1473 W_UnicodeObject
/// ._strip` — extract the optional `chars` argument as a `&str`,
/// raising TypeError on non-str non-None arguments rather than
/// silently falling through to the whitespace default.
fn extract_strip_chars(arg: PyObjectRef, fn_name: &str) -> Result<Option<String>, crate::PyError> {
    if arg.is_null() || unsafe { pyre_object::is_none(arg) } {
        return Ok(None);
    }
    if unsafe { pyre_object::is_str(arg) } {
        return Ok(Some(
            unsafe { pyre_object::w_str_get_value(arg) }.to_string(),
        ));
    }
    Err(crate::PyError::type_error(format!(
        "{fn_name} arg must be None or str"
    )))
}

pub fn str_method_strip(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let chars = match args.get(1) {
        Some(&a) => extract_strip_chars(a, "strip")?,
        None => None,
    };
    Ok(w_str_new(&strip_chars(s, chars.as_deref(), true, true)))
}

pub fn str_method_lstrip(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let chars = match args.get(1) {
        Some(&a) => extract_strip_chars(a, "lstrip")?,
        None => None,
    };
    Ok(w_str_new(&strip_chars(s, chars.as_deref(), true, false)))
}

pub fn str_method_rstrip(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let chars = match args.get(1) {
        Some(&a) => extract_strip_chars(a, "rstrip")?,
        None => None,
    };
    Ok(w_str_new(&strip_chars(s, chars.as_deref(), false, true)))
}

pub fn str_method_startswith(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let prefix = unsafe { w_str_get_value(args[1]) };
    Ok(w_bool_from(s.starts_with(prefix)))
}

pub fn str_method_endswith(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let suffix = unsafe { w_str_get_value(args[1]) };
    Ok(w_bool_from(s.ends_with(suffix)))
}

pub fn str_method_replace(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 3);
    let s = unsafe { w_str_get_value(args[0]) };
    let old = unsafe { w_str_get_value(args[1]) };
    let new = unsafe { w_str_get_value(args[2]) };
    Ok(w_str_new(&s.replace(old, new)))
}

pub fn str_method_find(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let sub = unsafe { w_str_get_value(args[1]) };
    Ok(w_int_new(s.find(sub).map(|i| i as i64).unwrap_or(-1)))
}

pub fn str_method_rfind(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let sub = unsafe { w_str_get_value(args[1]) };
    Ok(w_int_new(s.rfind(sub).map(|i| i as i64).unwrap_or(-1)))
}

pub fn str_method_upper(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    Ok(w_str_new(
        &unsafe { w_str_get_value(args[0]) }.to_uppercase(),
    ))
}

pub fn str_method_lower(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    Ok(w_str_new(
        &unsafe { w_str_get_value(args[0]) }.to_lowercase(),
    ))
}

/// PyPy: unicodeobject.py descr_format
/// Requires format spec parser — correct for no-arg case only.
/// `str.format(*args)` — PyPy: unicodeobject.py descr_format → newformat.py
pub fn str_method_format(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    // `pypy/objspace/std/newformat.py W_StringFormatter.format` —
    // positional args are slots 1.. of the receiver; keyword args
    // (`{name}` lookups) live in the trailing CALL_KW dict.
    let (positional, kwargs_dict) = crate::builtins::split_builtin_kwargs(&args[1..]);
    str_method_format_core(args[0], positional, kwargs_dict, None)
}

/// Shared core for `str.format` (`{name}` looks up the trailing
/// CALL_KW dict) and `str.format_map` (`{name}` looks up the
/// mapping via `space.getitem(mapping, w_key)`).  PyPy folds both
/// into one `newformat.format_method(space, fmt, args_w, w_kwds, ...)`
/// entry point per `unicodeobject.py:422-430`; pyre splits the
/// keyword-source into "dict snapshot" vs "lazy mapping" so the
/// mapping path stays line-by-line lazy (no pre-materialisation).
fn str_method_format_core(
    fmt_obj: PyObjectRef,
    positional: &[PyObjectRef],
    kwargs_dict: Option<PyObjectRef>,
    mapping: Option<PyObjectRef>,
) -> Result<PyObjectRef, crate::PyError> {
    let fmt = unsafe { pyre_object::w_str_get_value(fmt_obj) };
    let lookup_kwarg = |name: &str| -> Result<Option<PyObjectRef>, crate::PyError> {
        if let Some(m) = mapping {
            // `newformat.format_method(... w_mapping, True)` resolves
            // `{name}` via `space.getitem(mapping, w_key)` per
            // `newformat.py:Template.get_value`; KeyError propagates
            // to the caller (no silent default).
            let w_key = pyre_object::w_str_new(name);
            return crate::baseobjspace::getitem(m, w_key).map(Some);
        }
        if let Some(dict) = kwargs_dict {
            let v = unsafe { pyre_object::w_dict_lookup(dict, pyre_object::w_str_new(name)) };
            return Ok(v);
        }
        Ok(None)
    };

    let mut result = String::new();
    let mut auto_idx = 0usize;
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                result.push('{');
                i += 2;
                continue;
            }
            i += 1;
            let mut field = String::new();
            let mut spec = String::new();
            let mut in_spec = false;
            while i < bytes.len() && bytes[i] != b'}' {
                if bytes[i] == b':' && !in_spec {
                    in_spec = true;
                    i += 1;
                    continue;
                }
                if in_spec {
                    spec.push(bytes[i] as char);
                } else {
                    field.push(bytes[i] as char);
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }

            let val = if field.is_empty() {
                let idx = auto_idx;
                auto_idx += 1;
                positional
                    .get(idx)
                    .copied()
                    .unwrap_or(pyre_object::w_none())
            } else if let Ok(idx) = field.parse::<usize>() {
                positional
                    .get(idx)
                    .copied()
                    .unwrap_or(pyre_object::w_none())
            } else {
                lookup_kwarg(&field)?.unwrap_or(pyre_object::w_none())
            };
            let formatted = if spec.is_empty() {
                unsafe { crate::py_str(val) }
            } else {
                format_with_spec(val, &spec)
            };
            result.push_str(&formatted);
        } else if bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
            result.push('}');
            i += 2;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(pyre_object::w_str_new(&result))
}

/// Mini Python format-spec parser — `pypy/objspace/std/newformat.py
/// _parse_spec`.  Recognises the subset pyre exercises today:
/// `[fill][align][sign][#][0][width][.precision][type]`.  `alt_form`
/// (the `#` flag) is now stored on the parsed spec so int / float
/// formatters can apply the base prefix or trailing-zero
/// preservation per PyPy `newformat.py:454-468`.
struct ParsedSpec {
    fill: char,
    align: Option<char>,
    sign: Option<char>,
    alt_form: bool,
    zero_pad: bool,
    width: usize,
    precision: Option<usize>,
    ty: char,
}

fn parse_spec(spec: &str) -> ParsedSpec {
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;
    let n = chars.len();
    let mut fill = ' ';
    let mut align: Option<char> = None;
    if n >= 2 && matches!(chars[1], '<' | '>' | '=' | '^') {
        fill = chars[0];
        align = Some(chars[1]);
        i = 2;
    } else if n >= 1 && matches!(chars[0], '<' | '>' | '=' | '^') {
        align = Some(chars[0]);
        i = 1;
    }
    let mut sign: Option<char> = None;
    if i < n && matches!(chars[i], '+' | '-' | ' ') {
        sign = Some(chars[i]);
        i += 1;
    }
    let mut alt_form = false;
    if i < n && chars[i] == '#' {
        alt_form = true;
        i += 1;
    }
    let mut zero_pad = false;
    if i < n && chars[i] == '0' {
        zero_pad = true;
        // `pypy/objspace/std/newformat.py:454-460` — the `0` flag
        // implies `fill='0', align='='` when no explicit fill /
        // align were provided.  Without this, `f"{-7:=05d}"`
        // would left-fill with spaces instead of zeros.
        if align.is_none() {
            align = Some('=');
        }
        if fill == ' ' {
            fill = '0';
        }
        i += 1;
    }
    let mut width = 0usize;
    while i < n && chars[i].is_ascii_digit() {
        width = width * 10 + (chars[i] as u8 - b'0') as usize;
        i += 1;
    }
    let mut precision: Option<usize> = None;
    if i < n && chars[i] == '.' {
        i += 1;
        let mut p = 0usize;
        while i < n && chars[i].is_ascii_digit() {
            p = p * 10 + (chars[i] as u8 - b'0') as usize;
            i += 1;
        }
        precision = Some(p);
    }
    let ty = if i < n { chars[i] } else { '\0' };
    ParsedSpec {
        fill,
        align,
        sign,
        alt_form,
        zero_pad,
        width,
        precision,
        ty,
    }
}

fn pad_to_width(body: String, fill: char, align: char, width: usize) -> String {
    if body.chars().count() >= width {
        return body;
    }
    let need = width - body.chars().count();
    match align {
        '<' => {
            let mut s = body;
            for _ in 0..need {
                s.push(fill);
            }
            s
        }
        '^' => {
            let left = need / 2;
            let right = need - left;
            let mut s = String::with_capacity(width);
            for _ in 0..left {
                s.push(fill);
            }
            s.push_str(&body);
            for _ in 0..right {
                s.push(fill);
            }
            s
        }
        // `pypy/objspace/std/newformat.py:454-468` — `=` alignment
        // splits the leading sign / base prefix from the digit body
        // and inserts fill BETWEEN them, so `f"{-7:=05d}"` renders
        // as `"-0007"` (sign then zeros then digits) instead of
        // `"000-7"`.  Numeric bodies the int / float paths emit
        // start with at most one sign char (`-`/`+`/space) followed
        // by an optional base prefix (`0x`/`0X`/`0o`/`0b`).
        '=' => {
            let mut chars = body.chars().peekable();
            let mut prefix = String::new();
            if let Some(&c) = chars.peek() {
                if c == '-' || c == '+' || c == ' ' {
                    prefix.push(c);
                    chars.next();
                }
            }
            // Optional alt-form base prefix: 0x / 0X / 0o / 0b.
            let rest_so_far: String = chars.clone().collect();
            if rest_so_far.len() >= 2 && rest_so_far.as_bytes()[0] == b'0' {
                let next = rest_so_far.as_bytes()[1];
                if matches!(next, b'x' | b'X' | b'o' | b'b') {
                    prefix.push('0');
                    prefix.push(next as char);
                    chars.next();
                    chars.next();
                }
            }
            let digits: String = chars.collect();
            let mut s = String::with_capacity(width);
            s.push_str(&prefix);
            for _ in 0..need {
                s.push(fill);
            }
            s.push_str(&digits);
            s
        }
        _ => {
            // Default `>` (right-align) for any unknown sigil.
            let mut s = String::with_capacity(width);
            for _ in 0..need {
                s.push(fill);
            }
            s.push_str(&body);
            s
        }
    }
}

/// Public entry point for the f-string `FormatWithSpec` opcode in
/// `eval.rs::format_with_spec`. Forwards to the same parser used by
/// `str.format` so both surfaces share the spec semantics.
pub fn format_with_spec_public(val: PyObjectRef, spec: &str) -> String {
    format_with_spec(val, spec)
}

fn format_with_spec(val: PyObjectRef, spec: &str) -> String {
    let p = parse_spec(spec);
    unsafe {
        if pyre_object::is_int(val) || pyre_object::is_bool(val) {
            let v = if pyre_object::is_bool(val) {
                pyre_object::w_bool_get_value(val) as i64
            } else {
                pyre_object::w_int_get_value(val)
            };
            // Float-style spec on int: coerce to f64 (matches CPython
            // `int.__format__('.3f')` behaviour).
            if matches!(p.ty, 'f' | 'F' | 'e' | 'E' | 'g' | 'G') {
                return format_float(v as f64, &p);
            }
            return format_int(v, &p);
        }
        if pyre_object::is_float(val) {
            let v = pyre_object::floatobject::w_float_get_value(val);
            return format_float(v, &p);
        }
        if pyre_object::is_str(val) {
            let body = pyre_object::w_str_get_value(val).to_string();
            let body = if let Some(prec) = p.precision {
                body.chars().take(prec).collect()
            } else {
                body
            };
            let align = p.align.unwrap_or('<');
            return pad_to_width(body, p.fill, align, p.width);
        }
        let body = crate::py_str(val);
        let align = p.align.unwrap_or('<');
        pad_to_width(body, p.fill, align, p.width)
    }
}

fn format_int(v: i64, p: &ParsedSpec) -> String {
    let abs = v.unsigned_abs();
    let digits = match p.ty {
        'x' => format!("{abs:x}"),
        'X' => format!("{abs:X}"),
        'o' => format!("{abs:o}"),
        'b' => format!("{abs:b}"),
        _ => format!("{abs}"),
    };
    let sign_char = if v < 0 {
        "-"
    } else {
        match p.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };
    // `pypy/objspace/std/newformat.py:454-460` — alt-form `#`
    // prepends the matching base prefix for x/X/o/b; ignored for
    // d / decimal.
    let alt_prefix = if p.alt_form {
        match p.ty {
            'x' => "0x",
            'X' => "0X",
            'o' => "0o",
            'b' => "0b",
            _ => "",
        }
    } else {
        ""
    };
    let body = format!("{sign_char}{alt_prefix}{digits}");
    // `pypy/objspace/std/newformat.py:454-468` — when zero_pad is
    // set, parse_spec already promoted `align = '='` and `fill =
    // '0'` so `pad_to_width` performs the sign-aware insertion.
    let align = p.align.unwrap_or('>');
    pad_to_width(body, p.fill, align, p.width)
}

fn format_float(v: f64, p: &ParsedSpec) -> String {
    let prec = p.precision.unwrap_or(6);
    // Always format on `v.abs()` so the sign is reattached exactly
    // once below.  Rust's `{:e}` / `{:E}` include the sign already,
    // which previously duplicated `-` for negative values; using
    // `abs()` consistently fixes that.
    let abs = v.abs();
    let body = match p.ty {
        // `pypy/objspace/std/newformat.py` `format_e_g_complex`
        // mirrors C printf — the exponent has an explicit sign and
        // is zero-padded to two digits ("e+02", "E-04").  Rust's
        // `{:e}` emits a sign-less, minimal exponent ("e2"), so
        // route through `normalise_exponent` here.
        'e' => crate::baseobjspace::normalise_exponent(&format!("{abs:.prec$e}"), false),
        'E' => crate::baseobjspace::normalise_exponent(&format!("{abs:.prec$E}"), true),
        'f' | 'F' => format!("{:.*}", prec, abs),
        'g' | 'G' | '\0' => {
            // Match `format_g_like` in `baseobjspace.rs`'s % path:
            // alt-form keeps trailing zeros + the dot; default trims.
            // Cheapest route here is to delegate to
            // `format_g_like` for both spec types.
            if p.precision.is_some() || p.alt_form {
                crate::baseobjspace::format_g_like(abs, prec, p.ty == 'G', p.alt_form)
            } else {
                format!("{}", abs)
            }
        }
        _ => format!("{}", abs),
    };
    let sign_char = if v.is_sign_negative() && !v.is_nan() {
        "-"
    } else {
        match p.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };
    // `pypy/objspace/std/newformat.py:464-468` — alt-form `#` keeps
    // the trailing dot (and trailing zeros) for floats so the
    // requested precision is visible even when the value is whole.
    // Cheapest expression: append `.` if missing.
    let body = if p.alt_form
        && !body.contains('.')
        && matches!(p.ty, 'e' | 'E' | 'f' | 'F' | 'g' | 'G' | '\0')
    {
        format!("{body}.")
    } else {
        body
    };
    let body = format!("{sign_char}{body}");
    // Same `=`/`'0'` promotion as `format_int`; pad_to_width does
    // the sign-aware insertion.
    let align = p.align.unwrap_or('>');
    pad_to_width(body, p.fill, align, p.width)
}

/// PyPy: unicodeobject.py descr_encode → encode_object.
/// For the common 'utf-8' / 'ascii' fast paths, returns the UTF-8 bytes
/// of the string. Other codecs fall through to a best-effort UTF-8 encoding.
pub fn str_method_encode(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    // encoding arg (optional, default utf-8)
    let encoding: String = if args.len() >= 2 {
        unsafe {
            if pyre_object::is_str(args[1]) {
                w_str_get_value(args[1]).to_string()
            } else {
                "utf-8".to_string()
            }
        }
    } else {
        "utf-8".to_string()
    };
    let enc_lower = encoding.to_ascii_lowercase().replace('_', "-");
    match enc_lower.as_str() {
        "utf-8" | "utf8" | "u8" => Ok(pyre_object::w_bytes_from_bytes(s.as_bytes())),
        "ascii" | "us-ascii" | "646" => {
            if s.is_ascii() {
                Ok(pyre_object::w_bytes_from_bytes(s.as_bytes()))
            } else {
                Err(crate::PyError::value_error(
                    "'ascii' codec can't encode character: ordinal not in range(128)",
                ))
            }
        }
        "latin-1" | "latin1" | "iso-8859-1" | "8859" => {
            let mut out = Vec::with_capacity(s.len());
            for ch in s.chars() {
                if (ch as u32) > 0xFF {
                    return Err(crate::PyError::value_error(
                        "'latin-1' codec can't encode character: ordinal not in range(256)",
                    ));
                }
                out.push(ch as u8);
            }
            Ok(pyre_object::w_bytes_from_bytes(&out))
        }
        _ => Ok(pyre_object::w_bytes_from_bytes(s.as_bytes())),
    }
}

pub fn str_method_isdigit(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_bool_from(
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()),
    ))
}

pub fn str_method_isdecimal(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_bool_from(
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c.is_numeric()),
    ))
}

pub fn str_method_isnumeric(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_bool_from(
        !s.is_empty() && s.chars().all(|c| c.is_numeric()),
    ))
}

pub fn str_method_istitle(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let mut cased = false;
    let mut prev_cased = false;
    for c in s.chars() {
        if c.is_uppercase() {
            if prev_cased {
                return Ok(w_bool_from(false));
            }
            prev_cased = true;
            cased = true;
        } else if c.is_lowercase() {
            if !prev_cased {
                return Ok(w_bool_from(false));
            }
            prev_cased = true;
            cased = true;
        } else {
            prev_cased = false;
        }
    }
    Ok(w_bool_from(cased))
}

pub fn str_method_isalpha(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_bool_from(
        !s.is_empty() && s.chars().all(|c| c.is_alphabetic()),
    ))
}

/// PyPy: unicodeobject.py descr_isidentifier
pub fn str_method_isidentifier(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_bool_from(is_identifier(s)))
}

/// Check if a string is a valid Python identifier.
/// Python 3 identifiers: ID_Start ID_Continue*
/// Simplified: accepts ASCII identifiers + Unicode letters/digits.
fn is_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// `pypy/objspace/std/unicodeobject.py W_UnicodeObject.descr_zfill`.
/// Pads with leading zeros up to `width`; when the string starts with
/// a sign character (`+`/`-`), the sign stays at the front and zeros
/// fill between it and the digits (`'-42'.zfill(5) == '-0042'`).
pub fn str_method_zfill(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let width = unsafe { w_int_get_value(args[1]) }.max(0) as usize;
    let len = s.chars().count();
    if len >= width {
        return Ok(args[0]);
    }
    let need = width - len;
    let mut chars = s.chars();
    let mut out = String::with_capacity(width);
    let first = chars.clone().next();
    if let Some(c) = first {
        if c == '+' || c == '-' {
            out.push(c);
            chars.next();
        }
    }
    for _ in 0..need {
        out.push('0');
    }
    out.extend(chars);
    Ok(w_str_new(&out))
}

/// PyPy: unicodeobject.py descr_count
pub fn str_method_count(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let sub = unsafe { w_str_get_value(args[1]) };
    Ok(w_int_new(s.matches(sub).count() as i64))
}

/// PyPy: unicodeobject.py descr_index
pub fn str_method_index(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let sub = unsafe { w_str_get_value(args[1]) };
    match s.find(sub) {
        Some(i) => Ok(w_int_new(i as i64)),
        None => panic!("ValueError: substring not found"),
    }
}

/// PyPy: unicodeobject.py descr_title
pub fn str_method_title(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let mut result = String::with_capacity(s.len());
    let mut prev_is_sep = true;
    for c in s.chars() {
        if prev_is_sep {
            for u in c.to_uppercase() {
                result.push(u);
            }
        } else {
            for l in c.to_lowercase() {
                result.push(l);
            }
        }
        prev_is_sep = !c.is_alphanumeric();
    }
    Ok(w_str_new(&result))
}

/// PyPy: unicodeobject.py descr_capitalize
pub fn str_method_capitalize(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let mut chars = s.chars();
    let result = match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            let lower: String = chars.flat_map(|c| c.to_lowercase()).collect();
            format!("{upper}{lower}")
        }
    };
    Ok(w_str_new(&result))
}

/// PyPy: unicodeobject.py descr_swapcase
pub fn str_method_swapcase(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let result: String = s
        .chars()
        .flat_map(|c| {
            if c.is_uppercase() {
                c.to_lowercase().collect::<Vec<_>>()
            } else {
                c.to_uppercase().collect::<Vec<_>>()
            }
        })
        .collect();
    Ok(w_str_new(&result))
}

/// PyPy: unicodeobject.py descr_center
pub fn str_method_center(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let width = unsafe { w_int_get_value(args[1]) } as usize;
    let fillchar = if args.len() > 2 {
        unsafe { w_str_get_value(args[2]) }
            .chars()
            .next()
            .unwrap_or(' ')
    } else {
        ' '
    };
    if s.len() >= width {
        return Ok(args[0]);
    }
    let total_pad = width - s.len();
    let left = total_pad / 2;
    let right = total_pad - left;
    let result = format!(
        "{}{}{}",
        fillchar.to_string().repeat(left),
        s,
        fillchar.to_string().repeat(right)
    );
    Ok(w_str_new(&result))
}

/// PyPy: unicodeobject.py descr_ljust
pub fn str_method_ljust(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let width = unsafe { w_int_get_value(args[1]) } as usize;
    let fillchar = if args.len() > 2 {
        unsafe { w_str_get_value(args[2]) }
            .chars()
            .next()
            .unwrap_or(' ')
    } else {
        ' '
    };
    if s.len() >= width {
        return Ok(args[0]);
    }
    let pad = fillchar.to_string().repeat(width - s.len());
    Ok(w_str_new(&format!("{s}{pad}")))
}

/// PyPy: unicodeobject.py descr_rjust
pub fn str_method_rjust(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let width = unsafe { w_int_get_value(args[1]) } as usize;
    let fillchar = if args.len() > 2 {
        unsafe { w_str_get_value(args[2]) }
            .chars()
            .next()
            .unwrap_or(' ')
    } else {
        ' '
    };
    if s.len() >= width {
        return Ok(args[0]);
    }
    let pad = fillchar.to_string().repeat(width - s.len());
    Ok(w_str_new(&format!("{pad}{s}")))
}

/// PyPy: unicodeobject.py descr_isspace
pub fn str_method_isspace(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_bool_from(
        !s.is_empty() && s.chars().all(|c| c.is_whitespace()),
    ))
}

/// PyPy: unicodeobject.py descr_isupper
pub fn str_method_isupper(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let has_cased = s.chars().any(|c| c.is_alphabetic());
    Ok(w_bool_from(
        has_cased
            && s.chars()
                .filter(|c| c.is_alphabetic())
                .all(|c| c.is_uppercase()),
    ))
}

/// PyPy: unicodeobject.py descr_islower
pub fn str_method_islower(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let has_cased = s.chars().any(|c| c.is_alphabetic());
    Ok(w_bool_from(
        has_cased
            && s.chars()
                .filter(|c| c.is_alphabetic())
                .all(|c| c.is_lowercase()),
    ))
}

/// PyPy: unicodeobject.py descr_isalnum
pub fn str_method_isalnum(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_bool_from(
        !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()),
    ))
}

/// PyPy: unicodeobject.py descr_isascii
pub fn str_method_isascii(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    Ok(w_bool_from(s.is_ascii()))
}

/// PyPy: unicodeobject.py descr_partition
pub fn str_method_partition(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let sep = unsafe { w_str_get_value(args[1]) };
    match s.find(sep) {
        Some(i) => Ok(w_tuple_new(vec![
            w_str_new(&s[..i]),
            w_str_new(sep),
            w_str_new(&s[i + sep.len()..]),
        ])),
        None => Ok(w_tuple_new(vec![args[0], w_str_new(""), w_str_new("")])),
    }
}

/// PyPy: unicodeobject.py descr_rpartition
pub fn str_method_rpartition(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let sep = unsafe { w_str_get_value(args[1]) };
    match s.rfind(sep) {
        Some(i) => Ok(w_tuple_new(vec![
            w_str_new(&s[..i]),
            w_str_new(sep),
            w_str_new(&s[i + sep.len()..]),
        ])),
        None => Ok(w_tuple_new(vec![w_str_new(""), w_str_new(""), args[0]])),
    }
}

/// PyPy: unicodeobject.py descr_splitlines.
/// Walks `\n`, `\r`, and `\r\n` boundaries explicitly so that
/// `keepends=True` retains the terminator on each emitted line and a
/// trailing `\n` does NOT produce an extra empty entry — matching
/// `'a\nb\n'.splitlines() == ['a', 'b']`.
pub fn str_method_splitlines(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let keepends = if args.len() > 1 {
        crate::baseobjspace::is_true(args[1])
    } else {
        false
    };
    let bytes = s.as_bytes();
    let mut parts: Vec<PyObjectRef> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
            let mut term_end = i + 1;
            if bytes[i] == b'\r' && term_end < bytes.len() && bytes[term_end] == b'\n' {
                term_end += 1;
            }
            let end = if keepends { term_end } else { i };
            let line = std::str::from_utf8(&bytes[start..end])
                .unwrap_or("")
                .to_string();
            parts.push(w_str_new(&line));
            start = term_end;
            i = term_end;
        } else {
            i += 1;
        }
    }
    if start < bytes.len() {
        let line = std::str::from_utf8(&bytes[start..])
            .unwrap_or("")
            .to_string();
        parts.push(w_str_new(&line));
    }
    Ok(w_list_new(parts))
}

/// PyPy: unicodeobject.py descr_removeprefix (Python 3.9+)
pub fn str_method_removeprefix(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let prefix = unsafe { w_str_get_value(args[1]) };
    if s.starts_with(prefix) {
        Ok(w_str_new(&s[prefix.len()..]))
    } else {
        Ok(args[0])
    }
}

/// PyPy: unicodeobject.py descr_removesuffix (Python 3.9+)
pub fn str_method_removesuffix(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let s = unsafe { w_str_get_value(args[0]) };
    let suffix = unsafe { w_str_get_value(args[1]) };
    if s.ends_with(suffix) {
        Ok(w_str_new(&s[..s.len() - suffix.len()]))
    } else {
        Ok(args[0])
    }
}

/// PyPy: unicodeobject.py descr_expandtabs
pub fn str_method_expandtabs(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let s = unsafe { w_str_get_value(args[0]) };
    let tabsize = if args.len() > 1 {
        (unsafe { w_int_get_value(args[1]) }) as usize
    } else {
        8
    };
    let result = s.replace('\t', &" ".repeat(tabsize));
    Ok(w_str_new(&result))
}

/// PyPy: unicodeobject.py descr_translate
///
/// str.translate(table) — table is a dict mapping ordinals (int) to
/// ordinals (int), strings (str), or None (delete).
pub fn str_method_translate(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2, "translate() takes exactly one argument");
    let s = unsafe { w_str_get_value(args[0]) };
    let table = args[1];
    let mut result = String::with_capacity(s.len());
    unsafe {
        for ch in s.chars() {
            let key = w_int_new(ch as i64);
            if let Some(val) = w_dict_lookup(table, key) {
                if is_none(val) {
                    // None → delete character
                } else if is_int(val) {
                    if let Some(c) = char::from_u32(w_int_get_value(val) as u32) {
                        result.push(c);
                    }
                } else if is_str(val) {
                    result.push_str(w_str_get_value(val));
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        }
    }
    Ok(w_str_new(&result))
}

// ── Dict methods ─────────────────────────────────────────────────────

/// Resolve the actual backing W_DictObject for either a plain dict or
/// a dict subclass instance (which stores data in `__dict_data__`).
///
/// PyPy: W_DictMultiObject subclass instances ARE dicts, so no indirection
/// is needed. In pyre, dict subclass instances are W_InstanceObject with a
/// backing dict stored as an attribute.
pub fn resolve_dict_backing(obj: PyObjectRef) -> PyObjectRef {
    unsafe {
        if is_dict(obj) {
            return obj;
        }
        // `pypy/objspace/std/dictproxyobject.py:75-82 keys_w/values_w/
        // items_w` forward through `space.call_method(self.w_mapping,
        // ...)` — the mapping is unwrapped before any dict-method
        // dispatch.  Surface the same shape here so
        // `dict_method_{keys,values,items,get,copy,update,...}` work
        // on `type.__dict__` without per-method proxy plumbing.
        if pyre_object::is_dict_proxy(obj) {
            let inner = pyre_object::w_dict_proxy_get_mapping(obj);
            if !inner.is_null() && pyre_object::is_dict(inner) {
                return inner;
            }
        }
        if is_instance(obj) {
            if let Ok(backing) = crate::baseobjspace::getattr(obj, "__dict_data__") {
                if is_dict(backing) {
                    return backing;
                }
            }
        }
    }
    pyre_object::PY_NULL
}

pub fn dict_method_get(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let dict = resolve_dict_backing(args[0]);
    let key = args[1];
    let default = args.get(2).copied().unwrap_or_else(w_none);
    if dict.is_null() {
        return Ok(default);
    }
    unsafe { Ok(w_dict_lookup(dict, key).unwrap_or(default)) }
}

/// `pypy/objspace/std/dictmultiobject.py:descr_keys` parity — returns
/// a live `dict_keys` view bound to the source dict, not a snapshot
/// list.  The view's iter / len / contains semantics dispatch back
/// through the source dict (see baseobjspace getattr arm) so
/// mutations on the dict are visible through the view, matching
/// `W_DictMultiViewKeysObject`'s behaviour.
pub fn dict_method_keys(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let dict = resolve_dict_backing(args[0]);
    if dict.is_null() {
        // Type-erased fallback: the receiver isn't a dict, surface
        // an empty view rather than fabricating a foreign-shaped
        // list (the view's source-dict slot tolerates PY_NULL via
        // the read-side guards).
        return Ok(pyre_object::dictviewobject::w_dict_view_new(
            pyre_object::PY_NULL,
            pyre_object::dictviewobject::DictViewKind::Keys,
        ));
    }
    Ok(pyre_object::dictviewobject::w_dict_view_new(
        dict,
        pyre_object::dictviewobject::DictViewKind::Keys,
    ))
}

/// `pypy/objspace/std/dictmultiobject.py:descr_values` parity — same
/// shape as `descr_keys`, kind tag `Values`.
pub fn dict_method_values(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let dict = resolve_dict_backing(args[0]);
    if dict.is_null() {
        return Ok(pyre_object::dictviewobject::w_dict_view_new(
            pyre_object::PY_NULL,
            pyre_object::dictviewobject::DictViewKind::Values,
        ));
    }
    Ok(pyre_object::dictviewobject::w_dict_view_new(
        dict,
        pyre_object::dictviewobject::DictViewKind::Values,
    ))
}

/// `pypy/objspace/std/dictmultiobject.py:descr_items` parity — same
/// shape as `descr_keys`, kind tag `Items`.
pub fn dict_method_items(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty());
    let dict = resolve_dict_backing(args[0]);
    if dict.is_null() {
        return Ok(pyre_object::dictviewobject::w_dict_view_new(
            pyre_object::PY_NULL,
            pyre_object::dictviewobject::DictViewKind::Items,
        ));
    }
    Ok(pyre_object::dictviewobject::w_dict_view_new(
        dict,
        pyre_object::dictviewobject::DictViewKind::Items,
    ))
}

/// Materialise a dict_keys / values / items view's current snapshot
/// as a list of items.  Mirrors the body of `W_DictMultiViewKeys
/// Object._iter_*` (`dictmultiobject.py:451-470`) — pyre's
/// view-iter dispatch (baseobjspace `__iter__` arm) calls this to
/// produce the iterator backing list, and `repr` / `len` go through
/// the same `w_dict_items` snapshot for consistency.
///
/// **PRE-EXISTING-ADAPTATION** — PyPy's `_iter_*` returns a *live*
/// `W_BaseDictIterator` (`dictmultiobject.py:1701-1741`) that walks
/// the dict's underlying storage on demand, so mutations to the
/// source dict during iteration surface (`RuntimeError: dictionary
/// changed size during iteration`).  Pyre snapshots the items eagerly
/// at iter-acquisition time, so a `dict.update()` in the middle of
/// `for k in d.keys()` is invisible to the iterator instead of
/// raising.  Convergence target: introduce live `W_DictViewKeys
/// Iterator` etc. backed by the dict storage's epoch counter, raising
/// when the source mutates.  Requires the DictStorage iterators to
/// gain a stable-identity cursor (currently `w_dict_items` clones
/// entries into a Vec).
pub fn dict_view_snapshot(view: PyObjectRef) -> Vec<PyObjectRef> {
    let kind = unsafe { pyre_object::dictviewobject::w_dict_view_get_kind(view) };
    let dict = unsafe { pyre_object::dictviewobject::w_dict_view_get_dict(view) };
    if dict.is_null() {
        return Vec::new();
    }
    let items = unsafe { pyre_object::w_dict_items(dict) };
    match kind {
        pyre_object::dictviewobject::DictViewKind::Keys => {
            items.into_iter().map(|(k, _)| k).collect()
        }
        pyre_object::dictviewobject::DictViewKind::Values => {
            items.into_iter().map(|(_, v)| v).collect()
        }
        pyre_object::dictviewobject::DictViewKind::Items => items
            .into_iter()
            .map(|(k, v)| w_tuple_new(vec![k, v]))
            .collect(),
    }
}

/// `pypy/objspace/std/dictmultiobject.py descr_copy` — shallow copy
/// returning a fresh `W_DictObject` with the same (k, v) pairs.  Used
/// by `dict.copy()` and (via `resolve_dict_backing` proxy unwrap) by
/// `mappingproxy.copy()` (`dictproxyobject.py:84 copy_w`).  Mirrors
/// the inline closure that previously lived in `init_dict_type`.
pub fn dict_method_copy(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    if args.is_empty() {
        return Ok(pyre_object::w_dict_new());
    }
    let src = resolve_dict_backing(args[0]);
    if src.is_null() {
        return Ok(pyre_object::w_dict_new());
    }
    let dst = pyre_object::w_dict_new();
    unsafe {
        // Walk the union view so storage-proxy entries (e.g. on a
        // `module.__dict__` source) make it into the copy alongside
        // entries Vec slots.
        for (k, v) in pyre_object::w_dict_items(src) {
            w_dict_store(dst, k, v);
        }
    }
    Ok(dst)
}

/// PyPy: dictobject.py descr_update — dict.update([other], **kwargs).
///
/// CPython 3.x signature accepts a single optional positional that is
/// either a mapping (uses keys()) or an iterable of (key, value) pairs,
/// followed by arbitrary kwargs that are merged on top.  The trailing
/// `__pyre_kw__`-marked dict is the kwargs vehicle pyre's CALL_KW
/// emits for builtin callees (`call.rs:727-744`).
pub fn dict_method_update(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(!args.is_empty(), "dict.update() needs the receiver");
    let (positional, kwargs_dict) = crate::builtins::split_builtin_kwargs(args);
    // `pypy/objspace/std/dictmultiobject.py:1428` —
    // `descr_update(self, w_other=None, **kwargs)`.  The signature
    // accepts at most one extra positional after `self`; PyPy's
    // gateway raises TypeError on the second one.  Pyre's flat ABI
    // would otherwise silently ignore positional args past index 1.
    if positional.len() > 2 {
        return Err(crate::PyError::type_error(format!(
            "update expected at most 1 argument, got {}",
            positional.len() - 1
        )));
    }
    let dict = resolve_dict_backing(positional[0]);
    if dict.is_null() {
        return Ok(w_none());
    }
    if let Some(other) = positional.get(1).copied() {
        let other_raw = resolve_dict_backing(other);
        unsafe {
            // `dictmultiobject.py:1380-1387 update1` — the
            // `W_DictMultiObject` fast path runs only when (a) `w_data`
            // is a real dict AND (b) its type's `__iter__` is still
            // `dict.__iter__`.  A subclass that overrides `__iter__`
            // must round-trip through the slower `keys()` branch so the
            // override is observable.
            let fast_path_eligible = !other_raw.is_null()
                && pyre_object::is_dict(other_raw)
                && dict_subclass_uses_default_iter(other);
            if fast_path_eligible {
                // `dictmultiobject.py:1382-1387 update1_dict_dict` —
                // fast path when the source is a real dict and no
                // subclass override is on the table.  Walk the union
                // view so updates from a storage-proxied source dict
                // (e.g. `module.__dict__`) include entries that live
                // only in the proxy storage.
                for (k, v) in pyre_object::w_dict_items(other_raw) {
                    w_dict_store(dict, k, v);
                }
            } else {
                // `dictmultiobject.py:1388-1398 update1` — when the
                // source has a `keys()` method, iterate the keys and
                // copy `o[k]` into the dict (the general
                // mapping-protocol path).  Otherwise fall through to
                // the iterable-of-pairs path.
                let w_keys_method = match crate::baseobjspace::getattr(other, "keys") {
                    Ok(value) => Some(value),
                    Err(e) if e.kind == crate::PyErrorKind::AttributeError => None,
                    Err(e) => return Err(e),
                };
                if let Some(w_method) = w_keys_method {
                    let w_keys_view = crate::call::call_function_impl_result(w_method, &[])?;
                    let keys = crate::builtins::collect_iterable(w_keys_view)?;
                    for k in keys {
                        let v = crate::baseobjspace::getitem(other, k)?;
                        w_dict_store(dict, k, v);
                    }
                } else {
                    // `dictmultiobject.py:1410-1416 update1_pairs` —
                    // unpack each item to exactly two elements.  Error
                    // message includes the element index per
                    // `:1414-1415 "dictionary update sequence element
                    // #%d has length %d; 2 is required"`.
                    let pairs = crate::builtins::collect_iterable(other)?;
                    for (idx, pair) in pairs.into_iter().enumerate() {
                        let entries = crate::builtins::collect_iterable(pair)?;
                        if entries.len() != 2 {
                            return Err(crate::PyError::value_error(format!(
                                "dictionary update sequence element #{idx} has length {}; 2 is required",
                                entries.len()
                            )));
                        }
                        w_dict_store(dict, entries[0], entries[1]);
                    }
                }
            }
        }
    }
    if let Some(kwargs) = kwargs_dict {
        unsafe {
            for (k, v) in pyre_object::w_dict_items(kwargs) {
                if pyre_object::is_str(k) && pyre_object::w_str_get_value(k) == "__pyre_kw__" {
                    continue;
                }
                w_dict_store(dict, k, v);
            }
        }
    }
    dict_sync_dict_storage_proxy(dict);
    Ok(w_none())
}

/// `dictmultiobject.py:1380-1386 update1` —
///
/// ```python
/// if (isinstance(w_data, W_DictMultiObject) and
///         space.is_w(
///             space.findattr(space.type(w_data), w_st_iter),
///             space.findattr(space.w_dict, w_st_iter))):
///     update1_dict_dict(space, w_dict, w_data)
/// ```
///
/// Returns True when `other` is either a real `dict` (no subclass)
/// or a `dict` subclass that hasn't shadowed `__iter__`.  Pyre's
/// `is_dict` already established the W_DictMultiObject side; this
/// helper performs the `findattr` identity check to keep
/// `__iter__`-overriding subclasses on the slow `keys()` path.
fn dict_subclass_uses_default_iter(other: PyObjectRef) -> bool {
    let Some(other_type) = crate::typedef::r#type(other) else {
        return false;
    };
    let dict_type = crate::typedef::gettypeobject(&pyre_object::DICT_TYPE);
    if dict_type.is_null() {
        // No registered dict typeobject yet (init order quirk) —
        // degrade to "fast path" (treat as plain dict) to preserve
        // current behaviour.
        return true;
    }
    // Real `dict` type — no subclass at all, so __iter__ is by
    // definition unshadowed.
    if std::ptr::eq(other_type as *const _, dict_type as *const _) {
        return true;
    }
    let other_iter = unsafe { crate::baseobjspace::lookup_in_type(other_type, "__iter__") };
    let dict_iter = unsafe { crate::baseobjspace::lookup_in_type(dict_type, "__iter__") };
    match (other_iter, dict_iter) {
        (Some(a), Some(b)) => std::ptr::eq(a, b),
        _ => false,
    }
}

/// If dict has a dict_storage_proxy (i.e. it was returned by `globals()`),
/// sync all str-keyed entries back to the DictStorage.
fn dict_sync_dict_storage_proxy(dict: PyObjectRef) {
    unsafe {
        let ns_ptr = pyre_object::w_dict_get_dict_storage_proxy(dict);
        if ns_ptr.is_null() {
            return;
        }
        let ns = &mut *(ns_ptr as *mut crate::DictStorage);
        let dict_obj = &*(dict as *const pyre_object::dictobject::W_DictObject);
        let entries = &*dict_obj.entries;
        for &(k, v) in entries {
            if pyre_object::is_str(k) {
                let name = pyre_object::w_str_get_value(k);
                crate::dict_storage_store(ns, name, v);
            }
        }
    }
}

/// PyPy: dictobject.py descr_pop
pub fn dict_method_pop(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2, "dict.pop() takes at least 1 argument");
    let dict = resolve_dict_backing(args[0]);
    let key = args[1];
    let default = args.get(2).copied();
    if !dict.is_null() {
        unsafe {
            if let Some(val) = w_dict_lookup(dict, key) {
                // `pypy/objspace/std/dictmultiobject.py
                // W_DictMultiObject.descr_pop` calls
                // `space.delitem(self, w_key)` after the lookup,
                // which dispatches to the storage's `delitem`
                // implementation — equality-based, key-type
                // agnostic.  pyre routes through `w_dict_delitem`
                // which walks entries with `dict_keys_equal`, so
                // `d.pop(int_key)` actually removes the entry
                // instead of the previous str-only branch silently
                // leaving the dict mutated only by happenstance.
                pyre_object::dictobject::w_dict_delitem(dict, key);
                return Ok(val);
            }
        }
    }
    if let Some(d) = default {
        Ok(d)
    } else {
        Err(crate::PyError::key_error("dict.pop(): key not found"))
    }
}

pub fn dict_method_setdefault(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2);
    let dict = resolve_dict_backing(args[0]);
    let key = args[1];
    let default = args.get(2).copied().unwrap_or_else(w_none);
    if !dict.is_null() {
        unsafe {
            if let Some(existing) = w_dict_lookup(dict, key) {
                return Ok(existing);
            }
            w_dict_store(dict, key, default);
        }
    }
    Ok(default)
}

// ── Tuple methods ────────────────────────────────────────────────────

/// PyPy: tupleobject.py descr_index — tuple.index(value)
pub fn tuple_method_index(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2, "index() takes at least 1 argument");
    let tup = args[0];
    let value = args[1];
    unsafe {
        let n = w_tuple_len(tup);
        for i in 0..n {
            if let Some(item) = w_tuple_getitem(tup, i as i64) {
                if std::ptr::eq(item, value) {
                    return Ok(w_int_new(i as i64));
                }
                if is_int(item) && is_int(value) && w_int_get_value(item) == w_int_get_value(value)
                {
                    return Ok(w_int_new(i as i64));
                }
            }
        }
    }
    panic!("ValueError: tuple.index(x): x not in tuple")
}

/// PyPy: tupleobject.py descr_count — tuple.count(value)
pub fn tuple_method_count(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    assert!(args.len() >= 2, "count() takes exactly 1 argument");
    let tup = args[0];
    let value = args[1];
    let mut count: i64 = 0;
    unsafe {
        let n = w_tuple_len(tup);
        for i in 0..n {
            if let Some(item) = w_tuple_getitem(tup, i as i64) {
                if std::ptr::eq(item, value)
                    || (is_int(item)
                        && is_int(value)
                        && w_int_get_value(item) == w_int_get_value(value))
                {
                    count += 1;
                }
            }
        }
    }
    Ok(w_int_new(count))
}
