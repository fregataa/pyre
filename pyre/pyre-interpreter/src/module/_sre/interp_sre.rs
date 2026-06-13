//! _sre module — SRE regex engine bridge.
//!
//! Uses sre-engine crate (RustPython's SRE bytecode interpreter) in
//! place of `rpython/rlib/rsre`; the object model follows
//! `pypy/module/_sre/interp_sre.py` (`W_SRE_Pattern` / `W_SRE_Match`
//! typed fields, `pyre_object::sreobject`).

use crate::{
    DictStorage, dict_storage_store, make_builtin_function, make_builtin_function_with_arity,
    make_module_builtin_function, make_module_builtin_function_with_arity,
};
use pyre_object::sreobject::{
    W_SRE_Match, W_SRE_Pattern, W_SRE_Scanner, is_sre_match, is_sre_pattern, is_sre_scanner,
    w_sre_match_get_span, w_sre_match_new, w_sre_pattern_new, w_sre_scanner_new,
};
use pyre_object::*;
use sre_engine::engine::{Request, SearchIter, State};

pub fn register_module(ns: &mut DictStorage) {
    dict_storage_store(ns, "MAGIC", w_int_new(20230612)); // SRE magic number
    dict_storage_store(ns, "CODESIZE", w_int_new(sre_engine::CODESIZE as i64));
    dict_storage_store(ns, "MAXREPEAT", w_int_new(sre_engine::MAXREPEAT as i64));
    dict_storage_store(ns, "MAXGROUPS", w_int_new(sre_engine::MAXGROUPS as i64));
    // _sre module-level functions: PyPy mixedmodule.py:111-116 wraps these
    // as BuiltinFunction so storing them on a user class does not bind self.
    dict_storage_store(
        ns,
        "compile",
        make_module_builtin_function("compile", sre_compile),
    );
    dict_storage_store(
        ns,
        "ascii_iscased",
        make_module_builtin_function_with_arity(
            "ascii_iscased",
            |args| {
                if args.is_empty() {
                    return Ok(w_bool_from(false));
                }
                let ch = unsafe { w_int_get_value(args[0]) } as u8 as char;
                Ok(w_bool_from(ch.is_ascii_alphabetic()))
            },
            1,
        ),
    );
    dict_storage_store(
        ns,
        "unicode_iscased",
        make_module_builtin_function_with_arity(
            "unicode_iscased",
            |args| {
                if args.is_empty() {
                    return Ok(w_bool_from(false));
                }
                let ch = char::from_u32(unsafe { w_int_get_value(args[0]) } as u32).unwrap_or('\0');
                Ok(w_bool_from(ch.is_alphabetic()))
            },
            1,
        ),
    );
    dict_storage_store(
        ns,
        "ascii_tolower",
        make_module_builtin_function_with_arity(
            "ascii_tolower",
            |args| {
                if args.is_empty() {
                    return Ok(w_int_new(0));
                }
                Ok(w_int_new(
                    (unsafe { w_int_get_value(args[0]) } as u8).to_ascii_lowercase() as i64,
                ))
            },
            1,
        ),
    );
    dict_storage_store(
        ns,
        "unicode_tolower",
        make_module_builtin_function_with_arity(
            "unicode_tolower",
            |args| {
                if args.is_empty() {
                    return Ok(w_int_new(0));
                }
                let c = char::from_u32(unsafe { w_int_get_value(args[0]) } as u32).unwrap_or('\0');
                Ok(w_int_new(c.to_lowercase().next().unwrap_or(c) as i64))
            },
            1,
        ),
    );
    dict_storage_store(
        ns,
        "getcodesize",
        make_module_builtin_function_with_arity(
            "getcodesize",
            |_| Ok(w_int_new(sre_engine::CODESIZE as i64)),
            0,
        ),
    );
    dict_storage_store(
        ns,
        "getlower",
        make_module_builtin_function_with_arity(
            "getlower",
            |args| {
                if args.is_empty() {
                    return Ok(w_int_new(0));
                }
                Ok(w_int_new(sre_engine::string::lower_unicode(
                    unsafe { w_int_get_value(args[0]) } as u32
                ) as i64))
            },
            2,
        ),
    );
    // The 're.Pattern' / 're.Match' W_TypeObjects are created with the
    // other builtin typedefs in `typedef.rs` (W_SRE_Pattern.typedef /
    // W_SRE_Match.typedef, interp_sre.py:641/:869); instances carry
    // `pyre_object::sreobject` typed payloads.
}

/// `args[1]` as the typed pattern receiver for getsets registered on
/// the 're.Pattern' typedef (`args[0]` is the descriptor).
fn sre_pattern_receiver(args: &[PyObjectRef]) -> Result<*const W_SRE_Pattern, crate::PyError> {
    let self_ = args.get(1).copied().unwrap_or(pyre_object::PY_NULL);
    if unsafe { is_sre_pattern(self_) } {
        Ok(self_ as *const W_SRE_Pattern)
    } else {
        Err(crate::PyError::type_error("descriptor is for 're.Pattern'"))
    }
}

/// `args[1]` as the typed match receiver for getsets registered on
/// the 're.Match' typedef.
fn sre_match_receiver(args: &[PyObjectRef]) -> Result<*const W_SRE_Match, crate::PyError> {
    let self_ = args.get(1).copied().unwrap_or(pyre_object::PY_NULL);
    if unsafe { is_sre_match(self_) } {
        Ok(self_ as *const W_SRE_Match)
    } else {
        Err(crate::PyError::type_error("descriptor is for 're.Match'"))
    }
}

/// W_SRE_Pattern.typedef (interp_sre.py:641-668): instance methods are
/// registered on the type so `pat.match(s)` binds `pat` as `self`,
/// plus the `flags` / `groupindex` / `groups` / `pattern` attribute
/// properties (interp_sre.py:662-667).
pub(crate) fn init_sre_pattern_type(ns: &mut DictStorage) {
    // interp_sre.py:649 `__new__ = interp2app(SRE_Pattern__new__)`.
    dict_storage_store(
        ns,
        "__new__",
        pyre_object::propertyobject::w_staticmethod_new(make_builtin_function(
            "__new__",
            sre_pattern_new,
        )),
    );
    dict_storage_store(
        ns,
        "match",
        make_builtin_function("match", sre_pattern_match),
    );
    dict_storage_store(
        ns,
        "fullmatch",
        make_builtin_function("fullmatch", sre_pattern_fullmatch),
    );
    dict_storage_store(
        ns,
        "search",
        make_builtin_function("search", sre_pattern_search),
    );
    dict_storage_store(
        ns,
        "findall",
        make_builtin_function("findall", sre_pattern_findall),
    );
    dict_storage_store(
        ns,
        "finditer",
        make_builtin_function("finditer", sre_pattern_finditer),
    );
    // interp_sre.py:659 `scanner = interp2app(W_SRE_Pattern.finditer_w)`
    // — CPython/PyPy expose the same iterator constructor under both names.
    dict_storage_store(
        ns,
        "scanner",
        make_builtin_function("scanner", sre_pattern_finditer),
    );
    dict_storage_store(ns, "sub", make_builtin_function("sub", sre_pattern_sub));
    dict_storage_store(ns, "subn", make_builtin_function("subn", sre_pattern_subn));
    dict_storage_store(
        ns,
        "split",
        make_builtin_function("split", sre_pattern_split),
    );
    // interp_sre.py:651-653 `__repr__`/`__copy__`/`__deepcopy__`
    // (copy_identity_w returns self — compiled patterns are immutable).
    dict_storage_store(
        ns,
        "__repr__",
        make_builtin_function("__repr__", sre_pattern_repr),
    );
    dict_storage_store(
        ns,
        "__copy__",
        make_builtin_function("__copy__", sre_pattern_copy),
    );
    dict_storage_store(
        ns,
        "__deepcopy__",
        make_builtin_function("__deepcopy__", sre_pattern_copy),
    );
    // interp_sre.py:655-657 value equality / hash.
    dict_storage_store(
        ns,
        "__eq__",
        make_builtin_function("__eq__", sre_pattern_eq),
    );
    dict_storage_store(
        ns,
        "__hash__",
        make_builtin_function("__hash__", sre_pattern_hash),
    );
    dict_storage_store(ns, "__weakref__", crate::typedef::weakref_descr());
    // interp_sre.py:667-668 `generic_alias_class_getitem` as classmethod.
    // pyre's GenericAlias surface is still the repo-wide type-subscript
    // stub, so this returns the bound class just like `getitem_type`'s
    // current fallback while preserving the PyPy descriptor shape.
    dict_storage_store(
        ns,
        "__class_getitem__",
        pyre_object::propertyobject::w_classmethod_new(make_builtin_function(
            "__class_getitem__",
            sre_class_getitem,
        )),
    );
    // interp_sre.py:662-663 `flags = interp_attrproperty('flags', ...,
    // wrapfn="newint")`.
    dict_storage_store(
        ns,
        "flags",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "flags",
                |args| Ok(w_int_new(unsafe { (*sre_pattern_receiver(args)?).flags })),
                2,
            ),
            "flags",
        ),
    );
    // interp_sre.py:664 `groupindex = GetSetProperty(fget_groupindex)`
    // (:202-206 — a dict groupindex is exposed through a dictproxy).
    dict_storage_store(
        ns,
        "groupindex",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "groupindex",
                |args| {
                    let w_groupindex = unsafe { (*sre_pattern_receiver(args)?).w_groupindex };
                    if unsafe { is_dict(w_groupindex) } {
                        return Ok(pyre_object::dictproxyobject::w_dict_proxy_new(w_groupindex));
                    }
                    Ok(w_groupindex)
                },
                2,
            ),
            "groupindex",
        ),
    );
    // interp_sre.py:665-666 `groups = interp_attrproperty('num_groups',
    // ..., wrapfn="newint")`.
    dict_storage_store(
        ns,
        "groups",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "groups",
                |args| Ok(w_int_new(unsafe { (*sre_pattern_receiver(args)?).num_groups })),
                2,
            ),
            "groups",
        ),
    );
    // interp_sre.py:667 `pattern = interp_attrproperty_w('w_pattern', ...)`.
    dict_storage_store(
        ns,
        "pattern",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "pattern",
                |args| Ok(unsafe { (*sre_pattern_receiver(args)?).w_pattern }),
                2,
            ),
            "pattern",
        ),
    );
}

/// W_SRE_Match.typedef (interp_sre.py:869-895): methods + the `re` /
/// `string` / `pos` / `endpos` / `lastgroup` / `lastindex` attribute
/// properties.
pub(crate) fn init_sre_match_type(ns: &mut DictStorage) {
    dict_storage_store(ns, "group", make_builtin_function("group", sre_match_group));
    dict_storage_store(
        ns,
        "groups",
        make_builtin_function("groups", sre_match_groups),
    );
    dict_storage_store(ns, "start", make_builtin_function("start", sre_match_start));
    dict_storage_store(ns, "end", make_builtin_function("end", sre_match_end));
    dict_storage_store(ns, "span", make_builtin_function("span", sre_match_span));
    // interp_sre.py:880 `groupdict = interp2app(W_SRE_Match.groupdict_w)`.
    dict_storage_store(
        ns,
        "groupdict",
        make_builtin_function("groupdict", sre_match_groupdict),
    );
    // interp_sre.py:876 `__getitem__ = interp2app(W_SRE_Match.descr_getitem)`.
    dict_storage_store(
        ns,
        "__getitem__",
        make_builtin_function("__getitem__", sre_match_getitem),
    );
    // interp_sre.py:884 `expand = interp2app(W_SRE_Match.expand_w)`.
    dict_storage_store(
        ns,
        "expand",
        make_builtin_function("expand", sre_match_expand),
    );
    // interp_sre.py:873-875 `__copy__`/`__deepcopy__`/`__repr__`
    // (copy_identity_w returns self — match results are immutable).
    dict_storage_store(
        ns,
        "__repr__",
        make_builtin_function("__repr__", sre_match_repr),
    );
    dict_storage_store(
        ns,
        "__copy__",
        make_builtin_function("__copy__", sre_match_copy),
    );
    dict_storage_store(
        ns,
        "__deepcopy__",
        make_builtin_function("__deepcopy__", sre_match_copy),
    );
    // interp_sre.py:887 `re = interp_attrproperty_w('srepat', ...)`.
    dict_storage_store(
        ns,
        "re",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "re",
                |args| Ok(unsafe { (*sre_match_receiver(args)?).w_srepat }),
                2,
            ),
            "re",
        ),
    );
    // interp_sre.py:888 `string = GetSetProperty(fget_string)` (:866-867).
    dict_storage_store(
        ns,
        "string",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "string",
                |args| Ok(unsafe { (*sre_match_receiver(args)?).w_string }),
                2,
            ),
            "string",
        ),
    );
    // interp_sre.py:889 `pos = GetSetProperty(fget_pos)` (:851-852).
    dict_storage_store(
        ns,
        "pos",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "pos",
                |args| Ok(w_int_new(unsafe { (*sre_match_receiver(args)?).pos })),
                2,
            ),
            "pos",
        ),
    );
    // interp_sre.py:890 `endpos = GetSetProperty(fget_endpos)` (:854-855).
    dict_storage_store(
        ns,
        "endpos",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "endpos",
                |args| Ok(w_int_new(unsafe { (*sre_match_receiver(args)?).endpos })),
                2,
            ),
            "endpos",
        ),
    );
    // interp_sre.py:891 `lastgroup = GetSetProperty(fget_lastgroup)`
    // (:831-839 — the group name from `w_indexgroup[lastindex]`).
    dict_storage_store(
        ns,
        "lastgroup",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "lastgroup",
                |args| {
                    let m = sre_match_receiver(args)?;
                    let lastindex = unsafe { (*m).lastindex };
                    if lastindex < 0 {
                        return Ok(w_none());
                    }
                    let w_indexgroup = unsafe { (*(*m).w_srepat.cast::<W_SRE_Pattern>()).w_indexgroup };
                    let found = unsafe {
                        if is_list(w_indexgroup) {
                            w_list_getitem(w_indexgroup, lastindex)
                        } else if is_tuple(w_indexgroup) {
                            w_tuple_getitem(w_indexgroup, lastindex)
                        } else {
                            None
                        }
                    };
                    Ok(found.unwrap_or_else(w_none))
                },
                2,
            ),
            "lastgroup",
        ),
    );
    // interp_sre.py:892 `lastindex = GetSetProperty(fget_lastindex)`
    // (:841-845).
    dict_storage_store(
        ns,
        "lastindex",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "lastindex",
                |args| {
                    let lastindex = unsafe { (*sre_match_receiver(args)?).lastindex };
                    if lastindex >= 0 {
                        Ok(w_int_new(lastindex))
                    } else {
                        Ok(w_none())
                    }
                },
                2,
            ),
            "lastindex",
        ),
    );
    // interp_sre.py:892 `regs = GetSetProperty(W_SRE_Match.fget_regs)`.
    dict_storage_store(
        ns,
        "regs",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity("regs", sre_match_regs, 2),
            "regs",
        ),
    );
    // interp_sre.py:894-895 `generic_alias_class_getitem` as classmethod.
    dict_storage_store(
        ns,
        "__class_getitem__",
        pyre_object::propertyobject::w_classmethod_new(make_builtin_function(
            "__class_getitem__",
            sre_class_getitem,
        )),
    );
}

/// W_SRE_Scanner.typedef (interp_sre.py:949-957): the finditer/scanner
/// iterator — `__iter__`/`__next__` plus the undocumented `match`/`search`
/// methods and the `pattern` attribute property.
pub(crate) fn init_sre_scanner_type(ns: &mut DictStorage) {
    dict_storage_store(
        ns,
        "__iter__",
        make_builtin_function("__iter__", sre_scanner_iter),
    );
    dict_storage_store(
        ns,
        "__next__",
        make_builtin_function("__next__", sre_scanner_next_w),
    );
    dict_storage_store(
        ns,
        "match",
        make_builtin_function("match", sre_scanner_match),
    );
    dict_storage_store(
        ns,
        "search",
        make_builtin_function("search", sre_scanner_search),
    );
    // interp_sre.py:955 `pattern = interp_attrproperty_w('srepat', ...)`.
    dict_storage_store(
        ns,
        "pattern",
        crate::typedef::make_getset_descriptor_named(
            make_builtin_function_with_arity(
                "pattern",
                |args| {
                    let self_ = args.get(1).copied().unwrap_or(pyre_object::PY_NULL);
                    if unsafe { is_sre_scanner(self_) } {
                        Ok(unsafe { (*(self_ as *const W_SRE_Scanner)).w_srepat })
                    } else {
                        Err(crate::PyError::type_error(
                            "descriptor is for '_sre.SRE_Scanner'",
                        ))
                    }
                },
                2,
            ),
            "pattern",
        ),
    );
}

/// _sre.compile(pattern, flags, code, groups, groupindex, indexgroup)
/// — `SRE_Pattern__new__` (interp_sre.py:614-639).
fn sre_compile(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    if args.len() < 3 {
        return Err(crate::PyError::type_error(
            "_sre.compile() requires at least 3 arguments",
        ));
    }
    let pattern = args[0];
    let flags = unsafe { w_int_get_value(args[1]) };
    let code_list = args[2];
    let groups = if args.len() > 3 {
        unsafe { w_int_get_value(args[3]) }
    } else {
        0
    };
    let groupindex = if args.len() > 4 {
        args[4]
    } else {
        w_dict_new()
    };
    let indexgroup = if args.len() > 5 {
        args[5]
    } else {
        w_tuple_new(vec![])
    };

    if !unsafe { is_none(pattern) }
        && !unsafe { is_str(pattern) }
        && !unsafe { pyre_object::bytesobject::is_bytes_like(pattern) }
    {
        return Err(crate::PyError::type_error(
            "first argument must be string, bytes-like object or None",
        ));
    }

    let code_vec = extract_code(code_list)?;
    let code_box: &'static [u32] = Box::leak(code_vec.into_boxed_slice());

    Ok(w_sre_pattern_new(
        pattern, flags, code_box, groups, groupindex, indexgroup,
    ))
}

fn sre_pattern_new(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    if args.len() < 4 {
        return Err(crate::PyError::type_error(
            "SRE_Pattern.__new__() requires subtype, pattern, flags and code",
        ));
    }
    sre_compile(&args[1..])
}

fn sre_class_getitem(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    Ok(args.first().copied().unwrap_or_else(w_none))
}

fn extract_code(obj: PyObjectRef) -> Result<Vec<u32>, crate::PyError> {
    let n = crate::baseobjspace::len_w(obj)?;
    let mut code = Vec::with_capacity(n.max(0) as usize);
    for i in 0..n {
        let w_item = crate::baseobjspace::getitem(obj, w_int_new(i))?;
        code.push(crate::baseobjspace::uint_w(w_item)? as u32);
    }
    Ok(code)
}

/// `srepat.code` reader — typecheck guards the raw cast (the methods
/// are reachable through `Pattern.match(non_pattern, ...)`).
fn get_code(pat: PyObjectRef) -> Option<&'static [u32]> {
    if !unsafe { is_sre_pattern(pat) } {
        return None;
    }
    let pat = pat as *const W_SRE_Pattern;
    unsafe {
        if (*pat).code.is_null() {
            return None;
        }
        Some(std::slice::from_raw_parts((*pat).code, (*pat).code_len))
    }
}

#[inline]
fn char_len(s: &str) -> usize {
    s.chars().count()
}

#[inline]
fn char_to_byte(s: &str, char_pos: usize) -> usize {
    if char_pos == 0 {
        return 0;
    }
    s.char_indices()
        .nth(char_pos)
        .map(|(byte_pos, _)| byte_pos)
        .unwrap_or_else(|| s.len())
}

fn char_slice(s: &str, start: i64, end: i64) -> Option<&str> {
    if start < 0 || end < start {
        return None;
    }
    let start = start as usize;
    let end = end as usize;
    if end > char_len(s) {
        return None;
    }
    Some(&s[char_to_byte(s, start)..char_to_byte(s, end)])
}

fn push_char_slice(out: &mut String, s: &str, start: usize, end: usize) {
    if let Some(piece) = char_slice(s, start as i64, end as i64) {
        out.push_str(piece);
    }
}

fn normalize_char_bounds(s: &str, pos: i64, endpos: i64) -> (usize, usize) {
    let len = char_len(s);
    let pos = (pos.max(0) as usize).min(len);
    let mut endpos = (endpos.max(0) as usize).min(len);
    if endpos < pos {
        endpos = pos;
    }
    (pos, endpos)
}

fn do_match(
    args: &[PyObjectRef],
    search: bool,
    match_all: bool,
) -> Result<PyObjectRef, crate::PyError> {
    let (args, kwargs) = crate::builtins::split_builtin_kwargs(args);
    if args.len() < 2 {
        return Err(crate::PyError::type_error("requires self and string"));
    }
    let pat = args[0];
    let string = args[1];
    let s = unsafe { w_str_get_value(string) };
    let code = get_code(pat).ok_or_else(|| crate::PyError::type_error("no compiled code"))?;

    let (pos, endpos) = normalize_char_bounds(
        s,
        arg_int_kw(args, 2, kwargs, "pos", 0),
        arg_int_kw(args, 3, kwargs, "endpos", char_len(s) as i64),
    );

    let req = Request::new(s, pos, endpos, code, match_all);
    let mut state = State::default();

    let matched = if search {
        state.search(req)
    } else {
        state.py_match(&req)
    };

    if matched {
        Ok(make_match(pat, string, &state, pos as i64, endpos as i64))
    } else {
        Ok(w_none())
    }
}

/// Flatten the engine marks into the span table `do_flatten_marks`
/// (interp_sre.py:84-98) would produce, with group 0 (the whole match)
/// prepended.  The table is sized by the pattern's `num_groups` (filled
/// with `(-1, -1)` before copying the marks); the engine only
/// materialises marks up to the last touched group.  The sre-engine
/// `&str` driver reports character positions; PyPy stores byte positions
/// internally and converts on exposure, so this is the equivalent external
/// index convention for pyre's engine.
fn flatten_spans(pat: PyObjectRef, state: &State) -> Vec<(i64, i64)> {
    let start = state.start;
    let end = state.cursor.position;
    let num_groups = unsafe { (*(pat as *const W_SRE_Pattern)).num_groups }.max(0) as usize;
    let marked_groups = state.marks.raw().len() / 2;
    let mut spans: Vec<(i64, i64)> = vec![(start as i64, end as i64)];
    for gi in 0..num_groups {
        if gi >= marked_groups {
            spans.push((-1, -1));
            continue;
        }
        let (gs, ge) = state.marks.get(gi);
        spans.push(match (gs.into_option(), ge.into_option()) {
            (Some(a), Some(b)) => (a as i64, b as i64),
            _ => (-1, -1),
        });
    }
    spans
}

/// `slice_w` (interp_sre.py:57-80) over character positions of `s` — the span
/// sliced out of the subject string, or `w_default` for an unmatched
/// group (span `(-1, -1)` or otherwise out of range).
fn slice_span(s: &str, span: (i64, i64), w_default: PyObjectRef) -> PyObjectRef {
    char_slice(s, span.0, span.1)
        .map(w_str_new)
        .unwrap_or(w_default)
}

/// Build the W_SRE_Match for a successful engine run — the
/// `W_SRE_Match(self, ctx)` constructions (e.g. interp_sre.py:286-288)
/// with the span table flattened eagerly (`flatten_marks`, :793-797).
fn make_match(
    pat: PyObjectRef,
    string: PyObjectRef,
    state: &State,
    pos: i64,
    endpos: i64,
) -> PyObjectRef {
    // `_last_index` (interp_sre.py:825-829); -1 plays None.
    let lastindex = {
        let li = state.marks.last_index();
        if li >= 0 { li as i64 } else { -1 }
    };
    let spans = flatten_spans(pat, state);
    let spans: &'static [(i64, i64)] = Box::leak(spans.into_boxed_slice());

    w_sre_match_new(pat, string, pos, endpos, lastindex, spans)
}

fn sre_pattern_match(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    do_match(args, false, false)
}
fn sre_pattern_fullmatch(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    do_match(args, false, true)
}
fn sre_pattern_search(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    do_match(args, true, false)
}

/// Read an optional positional int argument (pos/endpos/count), falling
/// back to `default` when absent or non-int.
fn arg_int(args: &[PyObjectRef], idx: usize, default: i64) -> i64 {
    match args.get(idx) {
        Some(&w) if unsafe { is_int(w) } => unsafe { w_int_get_value(w) },
        _ => default,
    }
}

/// Resolve an optional int argument (`pos`/`endpos`/`count`) that may be
/// supplied positionally or by keyword — the unwrap_spec binding the
/// gateway performs for these builtins (e.g. `match(w_string, pos=0,
/// endpos=sys.maxint)`, interp_sre.py:262).  `pos_args` must already have
/// the trailing `__pyre_kw__` dict stripped ([`split_builtin_kwargs`]).
fn arg_int_kw(
    pos_args: &[PyObjectRef],
    idx: usize,
    kwargs: Option<PyObjectRef>,
    name: &str,
    default: i64,
) -> i64 {
    if let Some(w) = crate::builtins::kwarg_get(kwargs, name) {
        if unsafe { is_int(w) } {
            return unsafe { w_int_get_value(w) };
        }
    }
    arg_int(pos_args, idx, default)
}

/// `findall_w` (interp_sre.py:339-365) — non-overlapping matches.  With no
/// groups the whole match is collected; with one group that group's text;
/// with two or more a tuple of the groups.  Unmatched groups become the
/// empty string (`w_emptystr`, :344-347).
fn sre_pattern_findall(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let (args, kwargs) = crate::builtins::split_builtin_kwargs(args);
    if args.len() < 2 {
        return Err(crate::PyError::type_error(
            "findall requires self and string",
        ));
    }
    let pat = args[0];
    let s = unsafe { w_str_get_value(args[1]) };
    let code = get_code(pat).ok_or_else(|| crate::PyError::type_error("no code"))?;
    let (pos, endpos) = normalize_char_bounds(
        s,
        arg_int_kw(args, 2, kwargs, "pos", 0),
        arg_int_kw(args, 3, kwargs, "endpos", char_len(s) as i64),
    );
    let num_groups = unsafe { (*(pat as *const W_SRE_Pattern)).num_groups }.max(0) as usize;
    let w_empty = w_str_new("");

    let req = Request::new(s, pos, endpos, code, false);
    let mut iter = SearchIter {
        req,
        state: State::default(),
    };
    let mut results = Vec::new();
    while iter.next().is_some() {
        let spans = flatten_spans(pat, &iter.state);
        let w_item = if num_groups == 0 {
            slice_span(s, spans[0], w_empty)
        } else if num_groups == 1 {
            slice_span(s, spans[1], w_empty)
        } else {
            let grps: Vec<PyObjectRef> =
                (1..=num_groups).map(|g| slice_span(s, spans[g], w_empty)).collect();
            w_tuple_new(grps)
        };
        results.push(w_item);
    }
    Ok(w_list_new(results))
}

/// `finditer_w` (interp_sre.py:368-376) — returns the lazy
/// `W_SRE_Scanner` that yields a `W_SRE_Match` per non-overlapping match.
fn sre_pattern_finditer(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let (args, kwargs) = crate::builtins::split_builtin_kwargs(args);
    if args.len() < 2 {
        return Err(crate::PyError::type_error(
            "finditer requires self and string",
        ));
    }
    let pat = args[0];
    let string = args[1];
    if !unsafe { is_sre_pattern(pat) } {
        return Err(crate::PyError::type_error("descriptor 'finditer' for 're.Pattern'"));
    }
    let s = unsafe { w_str_get_value(string) };
    // Validate the compiled code is present (matches do_match's guard).
    get_code(pat).ok_or_else(|| crate::PyError::type_error("no compiled code"))?;
    let (pos, endpos) = normalize_char_bounds(
        s,
        arg_int_kw(args, 2, kwargs, "pos", 0),
        arg_int_kw(args, 3, kwargs, "endpos", char_len(s) as i64),
    );
    Ok(w_sre_scanner_new(
        pat,
        string,
        pos as i64,
        endpos as i64,
    ))
}

/// `sub_w` (interp_sre.py:409-412).
fn sre_pattern_sub(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let (w_item, _n) = subx(args)?;
    Ok(w_item)
}

/// `subn_w` (interp_sre.py:415-419) — returns `(new_string, count)`.
fn sre_pattern_subn(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let (w_item, n) = subx(args)?;
    Ok(w_tuple_new(vec![w_item, w_int_new(n)]))
}

/// `subx` (interp_sre.py:421-558) — the shared sub/subn body.  `repl` is a
/// callable (invoked per match), a literal template string with backslash
/// references expanded ([`parse_template`]), or a plain literal; `count`
/// caps the number of substitutions (0 = unlimited).
fn subx(args: &[PyObjectRef]) -> Result<(PyObjectRef, i64), crate::PyError> {
    let (args, kwargs) = crate::builtins::split_builtin_kwargs(args);
    if args.len() < 3 {
        return Err(crate::PyError::type_error("sub requires self, repl, string"));
    }
    let pat = args[0];
    let w_repl = args[1];
    let string = args[2];
    let s = unsafe { w_str_get_value(string) };
    let code = get_code(pat).ok_or_else(|| crate::PyError::type_error("no compiled code"))?;
    let count = arg_int_kw(args, 3, kwargs, "count", 0);

    // interp_sre.py:437-472 — a callable filter is applied per match; a
    // literal (no backslash) is inserted verbatim; otherwise the template
    // is compiled into a literal/group reference list.
    let filter_is_callable = crate::baseobjspace::callable_w(w_repl);
    let template = if filter_is_callable {
        None
    } else {
        if !unsafe { is_str(w_repl) } {
            return Err(crate::PyError::type_error(
                "sub: replacement must be str or callable",
            ));
        }
        let r = unsafe { w_str_get_value(w_repl) };
        Some(parse_template(r, pat as *const W_SRE_Pattern)?)
    };

    let mut result = String::new();
    let mut n: i64 = 0;
    let mut last = 0usize;
    let endpos = char_len(s);
    let req = Request::new(s, 0, endpos, code, false);
    let mut iter = SearchIter {
        req,
        state: State::default(),
    };
    while iter.next().is_some() {
        if count > 0 && n >= count {
            break;
        }
        let mstart = iter.state.start;
        let mend = iter.state.cursor.position;
        // interp_sre.py:499-502 — copy the gap before this match.
        push_char_slice(&mut result, s, last, mstart);
        last = mend;
        if let Some(items) = &template {
            let m = make_match(pat, string, &iter.state, 0, endpos as i64);
            expand_into(&mut result, items, m as *const W_SRE_Match, s);
        } else {
            // interp_sre.py:505-513 — callable filter; None means "no
            // piece" (treated as empty), otherwise the returned str.
            let m = make_match(pat, string, &iter.state, 0, endpos as i64);
            let w_piece = crate::baseobjspace::call_function(w_repl, &[m]);
            if w_piece.is_null() {
                return Err(crate::call::take_call_error()
                    .unwrap_or_else(|| crate::PyError::runtime_error("sub callable failed")));
            }
            if !unsafe { is_none(w_piece) } {
                if !unsafe { is_str(w_piece) } {
                    return Err(crate::PyError::type_error(
                        "sub callable must return a string",
                    ));
                }
                result.push_str(unsafe { w_str_get_value(w_piece) });
            }
        }
        n += 1;
    }
    // interp_sre.py:535-537 — append the trailing gap.
    push_char_slice(&mut result, s, last, endpos);
    Ok((w_str_new(&result), n))
}

fn sre_pattern_split(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    if args.len() < 2 {
        return Ok(w_list_new(vec![]));
    }
    Ok(w_list_new(vec![args[1]])) // stub
}

// ── Replacement-template parser (`re._parser.parse_template`) ──────────

/// A parsed replacement-template element: either a literal run or a
/// reference to group `n` (0 = whole match) — `parse_template`'s result
/// list (`re/_parser.py:990-1066`).
enum TemplateItem {
    Literal(String),
    Group(usize),
}

#[inline]
fn is_octdigit(c: char) -> bool {
    ('0'..='7').contains(&c)
}

/// Flush the accumulated literal then append a validated group reference —
/// `parse_template.addgroup` (`re/_parser.py:1006-1010`).  A reference
/// past `num_groups` is an "invalid group reference".
fn push_group(
    result: &mut Vec<TemplateItem>,
    literal: &mut String,
    index: i64,
    num_groups: i64,
) -> Result<(), crate::PyError> {
    if index < 0 || index > num_groups {
        return Err(crate::PyError::value_error(format!(
            "invalid group reference {index}"
        )));
    }
    if !literal.is_empty() {
        result.push(TemplateItem::Literal(std::mem::take(literal)));
    }
    result.push(TemplateItem::Group(index as usize));
    Ok(())
}

/// `re._parser.parse_template` (`re/_parser.py:990-1066`) — split a
/// replacement string into literal runs and group references.  `\g<name>`
/// resolves names through the pattern's `groupindex`; `\1`..`\99` are
/// group references (or 3-digit octal escapes); `\0NN` is octal;
/// `\a\b\f\n\r\t\v\\` are character escapes; other letter escapes are an
/// error and other punctuation escapes are kept verbatim.
fn parse_template(
    template: &str,
    pat: *const W_SRE_Pattern,
) -> Result<Vec<TemplateItem>, crate::PyError> {
    let num_groups = unsafe { (*pat).num_groups }.max(0);
    let w_groupindex = unsafe { (*pat).w_groupindex };
    let chars: Vec<char> = template.chars().collect();
    let n = chars.len();
    let mut i = 0usize;
    let mut result: Vec<TemplateItem> = Vec::new();
    let mut literal = String::new();

    while i < n {
        let c = chars[i];
        if c != '\\' {
            literal.push(c);
            i += 1;
            continue;
        }
        i += 1; // consume the backslash
        if i >= n {
            return Err(crate::PyError::value_error("bad escape (end of pattern)"));
        }
        let c1 = chars[i];
        i += 1;
        if c1 == 'g' {
            // `\g<name>` / `\g<number>`.
            if i >= n || chars[i] != '<' {
                return Err(crate::PyError::value_error("missing <"));
            }
            i += 1;
            let mut name = String::new();
            loop {
                if i >= n {
                    return Err(crate::PyError::value_error(
                        "missing >, unterminated name",
                    ));
                }
                let ch = chars[i];
                i += 1;
                if ch == '>' {
                    break;
                }
                name.push(ch);
            }
            if name.is_empty() {
                return Err(crate::PyError::value_error("missing group name"));
            }
            let index: i64 = if name.chars().all(|c| c.is_ascii_digit()) {
                name.parse::<i64>()
                    .map_err(|_| crate::PyError::value_error("invalid group reference"))?
            } else {
                let w_name = w_str_new(&name);
                let found = if unsafe { is_dict(w_groupindex) } {
                    unsafe { pyre_object::w_dict_lookup(w_groupindex, w_name) }
                } else {
                    None
                };
                match found {
                    Some(w_num) => unsafe { w_int_get_value(w_num) },
                    None => {
                        return Err(crate::PyError::index_error(format!(
                            "unknown group name {name:?}"
                        )));
                    }
                }
            };
            push_group(&mut result, &mut literal, index, num_groups)?;
        } else if c1 == '0' {
            // Octal escape `\0`, `\0N`, `\0NN`.
            let mut octal = String::from("0");
            if i < n && is_octdigit(chars[i]) {
                octal.push(chars[i]);
                i += 1;
                if i < n && is_octdigit(chars[i]) {
                    octal.push(chars[i]);
                    i += 1;
                }
            }
            let val = (i64::from_str_radix(&octal, 8).unwrap_or(0) & 0xff) as u32;
            literal.push(char::from_u32(val).unwrap_or('\0'));
        } else if c1.is_ascii_digit() {
            // `\1`..`\99` group reference, or a 3-digit octal escape.
            let mut num = String::new();
            num.push(c1);
            let mut isoctal = false;
            if i < n && chars[i].is_ascii_digit() {
                let c2 = chars[i];
                num.push(c2);
                i += 1;
                if is_octdigit(c1) && is_octdigit(c2) && i < n && is_octdigit(chars[i]) {
                    num.push(chars[i]);
                    i += 1;
                    isoctal = true;
                    let val = i64::from_str_radix(&num, 8).unwrap_or(0);
                    if val > 0o377 {
                        return Err(crate::PyError::value_error(format!(
                            "octal escape value \\{num} outside of range 0-0o377"
                        )));
                    }
                    literal.push(char::from_u32(val as u32).unwrap_or('\0'));
                }
            }
            if !isoctal {
                let index = num
                    .parse::<i64>()
                    .map_err(|_| crate::PyError::value_error("invalid group reference"))?;
                push_group(&mut result, &mut literal, index, num_groups)?;
            }
        } else {
            // Character escape (`ESCAPES`), kept-verbatim punctuation, or
            // an invalid letter escape.
            match c1 {
                'a' => literal.push('\u{07}'),
                'b' => literal.push('\u{08}'),
                'f' => literal.push('\u{0c}'),
                'n' => literal.push('\n'),
                'r' => literal.push('\r'),
                't' => literal.push('\t'),
                'v' => literal.push('\u{0b}'),
                '\\' => literal.push('\\'),
                _ => {
                    if c1.is_ascii_alphabetic() {
                        return Err(crate::PyError::value_error(format!("bad escape \\{c1}")));
                    }
                    literal.push('\\');
                    literal.push(c1);
                }
            }
        }
    }
    if !literal.is_empty() {
        result.push(TemplateItem::Literal(literal));
    }
    Ok(result)
}

/// Expand the parsed template against a match, appending into `out` — the
/// `_sre.template().expand()` substitution.  An unmatched group reference
/// contributes the empty string (`g(group) or empty`).
fn expand_into(out: &mut String, items: &[TemplateItem], m: *const W_SRE_Match, s: &str) {
    for item in items {
        match item {
            TemplateItem::Literal(lit) => out.push_str(lit),
            TemplateItem::Group(idx) => {
                let span = unsafe { w_sre_match_get_span(m as PyObjectRef, *idx) }
                    .unwrap_or((-1, -1));
                if let Some(piece) = char_slice(s, span.0, span.1) {
                    out.push_str(piece);
                }
            }
        }
    }
}

/// `args[0]` as the typed match receiver for the methods above (bound
/// through the typedef so `args[0]` is the match itself).
fn sre_match_self(args: &[PyObjectRef]) -> Result<*const W_SRE_Match, crate::PyError> {
    let self_ = args.first().copied().unwrap_or(pyre_object::PY_NULL);
    if unsafe { is_sre_match(self_) } {
        Ok(self_ as *const W_SRE_Match)
    } else {
        Err(crate::PyError::type_error("descriptor is for 're.Match'"))
    }
}

/// `do_span` (interp_sre.py:805-823): resolve a group argument — an
/// index, or a name looked up in `srepat.w_groupindex` — to its span.
/// Unknown groups raise IndexError("no such group").
fn do_span(m: *const W_SRE_Match, w_arg: Option<PyObjectRef>) -> Result<(i64, i64), crate::PyError> {
    let groupnum: i64 = match w_arg {
        None => 0,
        Some(w_arg) => {
            if unsafe { is_int(w_arg) } {
                unsafe { w_int_get_value(w_arg) }
            } else {
                // interp_sre.py:812-818 — non-int argument: look the
                // name up in `srepat.w_groupindex`; KeyError →
                // IndexError("no such group").
                let w_groupindex =
                    unsafe { (*(*m).w_srepat.cast::<W_SRE_Pattern>()).w_groupindex };
                let found = if unsafe { is_dict(w_groupindex) } {
                    unsafe { pyre_object::w_dict_lookup(w_groupindex, w_arg) }
                } else {
                    None
                };
                match found {
                    Some(w_groupnum) => unsafe { w_int_get_value(w_groupnum) },
                    None => return Err(crate::PyError::index_error("no such group")),
                }
            }
        }
    };
    if groupnum < 0 {
        return Err(crate::PyError::index_error("no such group"));
    }
    unsafe { w_sre_match_get_span(m as PyObjectRef, groupnum as usize) }
        .ok_or_else(|| crate::PyError::index_error("no such group"))
}

/// `slice_w` (interp_sre.py): the span sliced out of the subject
/// string, or `w_default` for an unmatched group.
unsafe fn slice_w(m: *const W_SRE_Match, span: (i64, i64), w_default: PyObjectRef) -> PyObjectRef {
    let s = unsafe { w_str_get_value((*m).w_string) };
    slice_span(s, span, w_default)
}

/// `group_w` (interp_sre.py:708-726).
fn sre_match_group(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    let group_args = &args[1..];
    if group_args.len() <= 1 {
        let span = do_span(m, group_args.first().copied())?;
        return Ok(unsafe { slice_w(m, span, w_none()) });
    }
    let mut results = Vec::with_capacity(group_args.len());
    for &w_arg in group_args {
        let span = do_span(m, Some(w_arg))?;
        results.push(unsafe { slice_w(m, span, w_none()) });
    }
    Ok(w_tuple_new(results))
}

/// `groups_w` (interp_sre.py:728-732) — pyre reads the flattened span
/// table directly; unmatched groups (span `(-1, -1)`) take the optional
/// `default` argument.
fn sre_match_groups(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    let w_default = args.get(1).copied().unwrap_or_else(w_none);
    let n = unsafe { (*m).spans_len };
    let mut groups = Vec::new();
    for gi in 1..n {
        let span = unsafe { w_sre_match_get_span(m as PyObjectRef, gi) }.unwrap_or((-1, -1));
        groups.push(unsafe { slice_w(m, span, w_default) });
    }
    Ok(w_tuple_new(groups))
}

/// `groupdict_w` (interp_sre.py:735-751) — name→group-text map built by
/// iterating `srepat.w_groupindex`; unmatched groups take `default`.
fn sre_match_groupdict(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    let w_default = args.get(1).copied().unwrap_or_else(w_none);
    let w_groupindex = unsafe { (*(*m).w_srepat.cast::<W_SRE_Pattern>()).w_groupindex };
    let w_dict = w_dict_new();
    if unsafe { is_dict(w_groupindex) } {
        for (w_key, w_value) in unsafe { pyre_object::dictmultiobject::w_dict_items(w_groupindex) } {
            let groupnum = unsafe { w_int_get_value(w_value) };
            let span = unsafe { w_sre_match_get_span(m as PyObjectRef, groupnum as usize) }
                .unwrap_or((-1, -1));
            let w_grp = unsafe { slice_w(m, span, w_default) };
            unsafe { pyre_object::dictmultiobject::w_dict_store(w_dict, w_key, w_grp) };
        }
    }
    Ok(w_dict)
}

/// `fget_regs` (interp_sre.py:853-864) — `((start, end), ...)` for group
/// 0..num_groups, matching what `span(i)` reports; unmatched is `(-1, -1)`.
fn sre_match_regs(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_receiver(args)?;
    let n = unsafe { (*m).spans_len };
    let mut regs = Vec::with_capacity(n);
    for gi in 0..n {
        let (start, end) = unsafe { w_sre_match_get_span(m as PyObjectRef, gi) }.unwrap_or((-1, -1));
        regs.push(w_tuple_new(vec![w_int_new(start), w_int_new(end)]));
    }
    Ok(w_tuple_new(regs))
}

/// `start_w` (interp_sre.py:758-761).
fn sre_match_start(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    let (start, _) = do_span(m, args.get(1).copied())?;
    Ok(w_int_new(start))
}

/// `end_w` (interp_sre.py:763-766).
fn sre_match_end(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    let (_, end) = do_span(m, args.get(1).copied())?;
    Ok(w_int_new(end))
}

/// `span_w` (interp_sre.py:768-771).
fn sre_match_span(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    let (start, end) = do_span(m, args.get(1).copied())?;
    Ok(w_tuple_new(vec![w_int_new(start), w_int_new(end)]))
}

/// `descr_getitem` (interp_sre.py:704-706) — `m[index]` resolves the
/// single group's span and slices the subject string.
fn sre_match_getitem(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    let span = do_span(m, args.get(1).copied())?;
    Ok(unsafe { slice_w(m, span, w_none()) })
}

/// `expand_w` (interp_sre.py:753-757) — substitute the parsed template
/// against this match.  Upstream delegates to `re._expand`; pyre expands
/// natively since it owns the template parser.
fn sre_match_expand(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    let w_template = args.get(1).copied().unwrap_or_else(w_none);
    if !unsafe { is_str(w_template) } {
        return Err(crate::PyError::type_error("expand: template must be str"));
    }
    let template = unsafe { w_str_get_value(w_template) };
    let pat = unsafe { (*m).w_srepat as *const W_SRE_Pattern };
    let items = parse_template(template, pat)?;
    let s = unsafe { w_str_get_value((*m).w_string) };
    let mut out = String::new();
    expand_into(&mut out, &items, m, s);
    Ok(w_str_new(&out))
}

/// `repr_w` (interp_sre.py:684-699) — `<re.Match object; span=(s, e),
/// match=R>` with `R` the repr of the whole match truncated to 50
/// characters.  Positions are character offsets because sre-engine's
/// `&str` driver reports character positions.
pub(crate) fn sre_match_repr_str(m: PyObjectRef) -> Result<String, crate::PyError> {
    let mp = m as *const W_SRE_Match;
    let span = unsafe { w_sre_match_get_span(m, 0) }.unwrap_or((-1, -1));
    let (start, end) = span;
    let s = unsafe { w_str_get_value((*mp).w_string) };
    let w_match_str = slice_span(s, span, w_none());
    let matchrepr: String = unsafe { crate::py_repr(w_match_str) }?
        .chars()
        .take(50)
        .collect();
    Ok(format!(
        "<re.Match object; span=({start}, {end}), match={matchrepr}>"
    ))
}

fn sre_match_repr(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    Ok(w_str_new(&sre_match_repr_str(m as PyObjectRef)?))
}

/// `copy_identity_w` (interp_sre.py:701-702) — match results are
/// immutable, so `__copy__`/`__deepcopy__` return self.
fn sre_match_copy(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let m = sre_match_self(args)?;
    Ok(m as PyObjectRef)
}

/// `args[0]` as the typed pattern receiver for pattern methods bound
/// through the typedef (so `args[0]` is the pattern itself).
fn sre_pattern_self(args: &[PyObjectRef]) -> Result<*const W_SRE_Pattern, crate::PyError> {
    let self_ = args.first().copied().unwrap_or(pyre_object::PY_NULL);
    if unsafe { is_sre_pattern(self_) } {
        Ok(self_ as *const W_SRE_Pattern)
    } else {
        Err(crate::PyError::type_error("descriptor is for 're.Pattern'"))
    }
}

// SRE flag bits (`sre_constants.py`) consulted by the pattern repr.
const SRE_FLAG_LOCALE: i64 = 4;
const SRE_FLAG_UNICODE: i64 = 32;
const SRE_FLAG_ASCII: i64 = 256;
const SRE_FLAG_NAMES: [&str; 9] = [
    "re.TEMPLATE",
    "re.IGNORECASE",
    "re.LOCALE",
    "re.MULTILINE",
    "re.DOTALL",
    "re.UNICODE",
    "re.VERBOSE",
    "re.DEBUG",
    "re.ASCII",
];

/// `repr_w` (interp_sre.py:153-178) — `re.compile(<pattern repr>, <flags>)`
/// with the pattern repr truncated to 200 characters and the flag bits
/// decoded into their `re.*` names (the implicit `re.UNICODE` on a known
/// unicode pattern is suppressed, :160-165).
pub(crate) fn sre_pattern_repr_str(pat: PyObjectRef) -> Result<String, crate::PyError> {
    let pp = pat as *const W_SRE_Pattern;
    let w_pattern = unsafe { (*pp).w_pattern };
    let u: String = unsafe { crate::py_repr(w_pattern) }?.chars().take(200).collect();

    let mut flags = unsafe { (*pp).flags };
    let is_known_unicode = unsafe { is_str(w_pattern) };
    if is_known_unicode
        && (flags & (SRE_FLAG_LOCALE | SRE_FLAG_UNICODE | SRE_FLAG_ASCII)) == SRE_FLAG_UNICODE
    {
        flags &= !SRE_FLAG_UNICODE;
    }
    let mut flag_items: Vec<String> = Vec::new();
    for (i, name) in SRE_FLAG_NAMES.iter().enumerate() {
        if flags & (1 << i) != 0 {
            flags -= 1 << i;
            flag_items.push((*name).to_string());
        }
    }
    if flags != 0 {
        flag_items.push(format!("0x{flags:x}"));
    }
    if flag_items.is_empty() {
        Ok(format!("re.compile({u})"))
    } else {
        Ok(format!("re.compile({u}, {})", flag_items.join("|")))
    }
}

fn sre_pattern_repr(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let pat = sre_pattern_self(args)?;
    Ok(w_str_new(&sre_pattern_repr_str(pat as PyObjectRef)?))
}

/// `descr_eq` (interp_sre.py:180-190): compare flags, compiled code, and
/// original pattern; groupindex/indexgroup are derived from the pattern.
fn sre_pattern_eq(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let pat = sre_pattern_self(args)?;
    let other = args.get(1).copied().unwrap_or(pyre_object::PY_NULL);
    if !unsafe { is_sre_pattern(other) } {
        return Ok(pyre_object::w_not_implemented());
    }
    let p = unsafe { &*pat };
    let q = unsafe { &*(other as *const W_SRE_Pattern) };
    let p_code = get_code(pat as PyObjectRef).unwrap_or(&[]);
    let q_code = get_code(other).unwrap_or(&[]);
    Ok(w_bool_from(
        p.flags == q.flags
            && p_code == q_code
            && crate::baseobjspace::eq_w(p.w_pattern, q.w_pattern),
    ))
}

/// `descr_hash` (interp_sre.py:193-199): hash the compiled code, flags,
/// and original pattern in the same structural order as PyPy.
fn sre_pattern_hash(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let pat = sre_pattern_self(args)?;
    let p = unsafe { &*pat };
    let mut x: i64 = 0x345678;
    for &c in get_code(pat as PyObjectRef).unwrap_or(&[]) {
        x = x.wrapping_mul(1_000_003) ^ c as i64;
    }
    x = x.wrapping_mul(1_000_003) ^ p.flags;
    x = x.wrapping_mul(1_000_003) ^ crate::baseobjspace::hash_w_strict(p.w_pattern)?;
    Ok(w_int_new(x))
}

/// `copy_identity_w` (interp_sre.py:150-151) — compiled patterns are
/// immutable, so `__copy__`/`__deepcopy__` return self.
fn sre_pattern_copy(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let pat = sre_pattern_self(args)?;
    Ok(pat as PyObjectRef)
}

// ── SRE_Scanner (finditer iterator) ───────────────────────────────────

/// `args[0]` as the typed scanner receiver.
fn sre_scanner_self(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let self_ = args.first().copied().unwrap_or(pyre_object::PY_NULL);
    if unsafe { is_sre_scanner(self_) } {
        Ok(self_)
    } else {
        Err(crate::PyError::type_error(
            "descriptor is for '_sre.SRE_Scanner'",
        ))
    }
}

/// `W_SRE_Scanner.getmatch` (interp_sre.py:935-947) — advance the scanner
/// one step and build the match.  `anchored` selects `py_match` (the
/// `match` method) over `search` (`__next__` / `search`).  Returns
/// `Ok(None)` and marks the scanner exhausted (`self.ctx = None`) when no
/// further match is found.
fn sre_scanner_step(
    obj: PyObjectRef,
    anchored: bool,
) -> Result<Option<PyObjectRef>, crate::PyError> {
    let sc = obj as *mut W_SRE_Scanner;
    let pos = unsafe { (*sc).pos };
    if pos < 0 {
        return Ok(None); // self.ctx is None
    }
    let w_srepat = unsafe { (*sc).w_srepat };
    let string = unsafe { (*sc).w_string };
    let original_pos = unsafe { (*sc).original_pos };
    let endpos = unsafe { (*sc).endpos }.max(0) as usize;
    let s = unsafe { w_str_get_value(string) };
    let code = get_code(w_srepat).ok_or_else(|| crate::PyError::type_error("no compiled code"))?;
    let must_advance = unsafe { (*sc).must_advance } != 0;

    let mut req = Request::new(s, pos as usize, endpos, code, false);
    req.must_advance = must_advance;
    let mut state = State::default();
    let found = if anchored {
        state.py_match(&req)
    } else {
        state.search(req)
    };
    if !found {
        unsafe { (*sc).pos = -1 }; // self.ctx = None
        return Ok(None);
    }
    // engine.rs:255-256 — thread (start, must_advance) for the next step.
    let new_must_advance = state.cursor.position == state.start;
    unsafe {
        (*sc).must_advance = new_must_advance as i64;
        (*sc).pos = state.cursor.position as i64;
    }
    Ok(Some(make_match(
        w_srepat,
        string,
        &state,
        original_pos,
        endpos as i64,
    )))
}

/// `next_w` (interp_sre.py:918-923) — also the host `space.next` step for
/// `for m in pat.finditer(...)`; raises StopIteration when exhausted.
pub fn sre_scanner_next(obj: PyObjectRef) -> Result<PyObjectRef, crate::PyError> {
    match sre_scanner_step(obj, false)? {
        Some(m) => Ok(m),
        None => Err(crate::PyError::stop_iteration()),
    }
}

/// `iter_w` (interp_sre.py:915-916) — returns self.
fn sre_scanner_iter(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    sre_scanner_self(args)
}

fn sre_scanner_next_w(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let self_ = sre_scanner_self(args)?;
    sre_scanner_next(self_)
}

/// `match_w` (interp_sre.py:925-928) — anchored match at the current
/// position; returns None when exhausted instead of raising.
fn sre_scanner_match(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let self_ = sre_scanner_self(args)?;
    Ok(sre_scanner_step(self_, true)?.unwrap_or_else(w_none))
}

/// `search_w` (interp_sre.py:930-933) — search from the current position;
/// returns None when exhausted instead of raising.
fn sre_scanner_search(args: &[PyObjectRef]) -> Result<PyObjectRef, crate::PyError> {
    let self_ = sre_scanner_self(args)?;
    Ok(sre_scanner_step(self_, false)?.unwrap_or_else(w_none))
}
