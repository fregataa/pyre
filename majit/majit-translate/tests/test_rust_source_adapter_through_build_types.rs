//! End-to-end composition test: Rust-AST adapter output flows through
//! `RPythonAnnotator::build_types`.
//!
//! This is the payoff test for plan step M2.5g (Position-2 adaptation
//! "Rust AST adapter into unchanged flowspace", see
//! `~/.claude/plans/annotator-monomorphization-tier1-abstract-lake.md`):
//! it proves that a `syn::ItemFn` fed through
//! [`build_host_function_from_rust`] produces a `(HostObject, PyGraph)`
//! pair that upstream's annotator pipeline consumes without
//! modification — the same `build_types` → `get_call_parameters`
//! → `FunctionDesc.specialize` → `cachedgraph` → `buildgraph`
//! → `translator.buildflowgraph` → `_prebuilt_graphs` short-circuit
//! already exercised in unit tests, but driven here from a real
//! adapter-built graph rather than a hand-rolled stub.
//!
//! Upstream analogue:
//!   `annrpython.py:73-97 build_types` dispatches through
//!   `get_call_parameters(function, args_s)` → `FunctionDesc.specialize`
//!   → `cachedgraph` → `buildgraph` → `translator.buildflowgraph(pyobj)`.
//!   The `_prebuilt_graphs` short-circuit at `translator.py:50-51`
//!   returns the graph we seeded without running `build_flow`.

use std::collections::HashMap;
use std::rc::Rc;

use majit_translate::annotator::annrpython::RPythonAnnotator;
use majit_translate::annotator::signature::AnnotationSpec;
use majit_translate::flowspace::model::{FunctionGraph, Hlvalue};
use majit_translate::flowspace::rust_source::build_flow_from_rust;
use majit_translate::translator::interactive::Translation;

fn parse_item_fn(src: &str) -> syn::ItemFn {
    syn::parse_str::<syn::ItemFn>(src).expect("test fixture must parse as a single ItemFn")
}

/// For the small `if c { } else { }; 1` fixture below, assert that
/// the single statement-position `if/else` join carries exactly the
/// pre-fork locals and no value-position tail slot. This keeps the
/// oracle tied to the construct under test instead of rejecting
/// legitimate value-position joins whose result later happens to be
/// unused.
fn assert_statement_if_else_join_has_only_merged_local_inputargs(
    graph: &FunctionGraph,
    expected_merged_locals: usize,
) {
    let blocks = graph.iterblocks();
    let mut incoming: HashMap<usize, usize> = HashMap::new();
    for block in &blocks {
        for exit in &block.borrow().exits {
            if let Some(target) = &exit.borrow().target {
                let ptr = Rc::as_ptr(target) as usize;
                *incoming.entry(ptr).or_insert(0) += 1;
            }
        }
    }
    let return_key = Rc::as_ptr(&graph.returnblock) as usize;
    let except_key = Rc::as_ptr(&graph.exceptblock) as usize;
    let mut join_keys = Vec::new();
    for block_rc in &blocks {
        let key = Rc::as_ptr(block_rc) as usize;
        if key == return_key || key == except_key {
            continue;
        }
        if incoming.get(&key).copied().unwrap_or(0) < 2 {
            continue;
        }
        join_keys.push(key);
    }
    assert_eq!(
        join_keys.len(),
        1,
        "statement-position fixture should have exactly one non-special join block"
    );
    let join_key = join_keys[0];
    let join_rc = blocks
        .iter()
        .find(|block| Rc::as_ptr(block) as usize == join_key)
        .expect("join key came from graph blocks");
    let join = join_rc.borrow();
    assert_eq!(
        join.inputargs.len(),
        expected_merged_locals,
        "statement-position if/else join must carry only the merged locals; \
         an extra inputarg would be a value-position tail slot"
    );
    for (idx, arg) in join.inputargs.iter().enumerate() {
        match arg {
            Hlvalue::Variable(v) => assert!(
                v.renamed(),
                "merged local inputarg[{idx}] should be a named local Variable, \
                 not an unnamed tail slot"
            ),
            other => panic!("merged local inputarg[{idx}] should be a Variable, got {other:?}"),
        }
    }
}

fn assert_value_position_if_else_join_has_tail_inputarg(graph: &FunctionGraph) {
    let blocks = graph.iterblocks();
    let mut incoming: HashMap<usize, usize> = HashMap::new();
    for block in &blocks {
        for exit in &block.borrow().exits {
            if let Some(target) = &exit.borrow().target {
                let ptr = Rc::as_ptr(target) as usize;
                *incoming.entry(ptr).or_insert(0) += 1;
            }
        }
    }
    let return_key = Rc::as_ptr(&graph.returnblock) as usize;
    let except_key = Rc::as_ptr(&graph.exceptblock) as usize;
    for block_rc in &blocks {
        let key = Rc::as_ptr(block_rc) as usize;
        if key == return_key || key == except_key || incoming.get(&key).copied().unwrap_or(0) < 2 {
            continue;
        }
        let block = block_rc.borrow();
        if block
            .inputargs
            .iter()
            .any(|arg| matches!(arg, Hlvalue::Variable(v) if !v.renamed()))
        {
            return;
        }
    }
    panic!("value-position if/else fixture should expose an unnamed tail inputarg at its join");
}

#[test]
fn adapter_output_flows_through_build_types_for_constant_return() {
    // `fn one() -> i64 { 1 }` — the simplest constant-return case the
    // feasibility probe (`annotator_monomorphization` plan, feasibility
    // probe findings #1) validated for the bytecode path. Re-validate
    // the same contract for the Rust-source adapter path.
    //
    // `Translation::from_rust_item_fn` bundles the adapter call +
    // `_prebuilt_graphs` seed per upstream `interactive.py:25-26`.
    let item = parse_item_fn("fn one() -> i64 { 1 }");
    let (t, host) = Translation::from_rust_item_fn(&item).expect("translation");

    let ann = RPythonAnnotator::new_with_translator(
        Some(std::rc::Rc::clone(&t.context)),
        None,
        None,
        false,
    );
    let result = ann
        .build_types(&host, &[], true, false)
        .expect("build_types must succeed");
    // Constant-int return resolves through the returnblock's Link args
    // (the constant-carrying path in the feasibility probe's case #1).
    // Accept any integer annotation — narrower assertions live in the
    // annotator's own unit tests.
    match result {
        Some(sv) => {
            let ty = format!("{sv:?}");
            assert!(
                ty.contains("Integer"),
                "expected SomeValue::Integer, got {ty}"
            );
        }
        None => panic!("build_types should return an annotation for a literal-return function"),
    }
}

#[test]
fn adapter_output_main_entry_point_populates_translator_entry_graph() {
    // Upstream `annrpython.py:87-88`:
    //   `if main_entry_point: self.translator.entry_point_graph = flowgraph`.
    // Confirms the hook the future codewriter wiring (plan M3.2) will
    // use to retrieve the specialized portal graph.
    let item = parse_item_fn("fn one() -> i64 { 1 }");
    let (t, host) = Translation::from_rust_item_fn(&item).expect("translation");

    let ann = RPythonAnnotator::new_with_translator(
        Some(std::rc::Rc::clone(&t.context)),
        None,
        None,
        false,
    );
    let _ = ann
        .build_types(&host, &[], true, true)
        .expect("build_types must succeed");
    assert!(
        ann.translator.entry_point_graph.borrow().is_some(),
        "main_entry_point=true must populate translator.entry_point_graph"
    );
}

#[test]
fn adapter_output_preserves_prebuilt_graph_pop_semantics() {
    // Upstream `translator.py:50-51` pops the entry out of
    // `_prebuilt_graphs` on first lookup. The adapter-produced pair
    // should obey the same contract — subsequent lookups go to
    // `build_flow`, which for an adapter-only HostObject has no
    // bytecode and therefore surfaces an error. This pins the
    // single-consumption contract without asserting the fallback error
    // text (which is an implementation detail of `buildflowgraph`).
    let item = parse_item_fn("fn one() -> i64 { 1 }");
    let (t, host) = Translation::from_rust_item_fn(&item).expect("translation");

    // Capture the prebuilt Rc so we can verify `buildflowgraph` returns
    // the same instance on the first hit.
    let prebuilt = t
        .context
        ._prebuilt_graphs
        .borrow()
        .get(&host)
        .cloned()
        .expect("seeded by Translation::from_rust_item_fn");

    let first = t
        .context
        .buildflowgraph(host.clone(), false)
        .expect("first");
    assert!(std::rc::Rc::ptr_eq(&first, &prebuilt));

    // After the pop, the entry is gone.
    assert!(!t.context._prebuilt_graphs.borrow().contains_key(&host));
}

#[test]
fn build_types_succeeds_on_value_position_if_else_plus_match_cascade() {
    // Drives `RPythonAnnotator::build_types` through a function that
    // exercises the M2.5e Position-2 adapter's value-position
    // composition: a `let` binding initialised by a *value-position*
    // `if-else` (the if-expression is the RHS of a let, so its tail
    // value flows into the local), followed by a wildcard-tailed
    // match cascade with literal arms.
    //
    // For the **statement-position** if-else parity check (no value
    // flows out of the if; the join carries only merged locals per
    // `framestate.py:33 mergeable = locals_w + stack`), see
    // `statement_position_if_else_join_has_only_merged_local_inputargs`
    // below.
    //
    // If a future change regresses the annotator's handling of any
    // single one of those constructs (e.g. forgetting to thread the
    // join-block of a value-position `if-else` into the subsequent
    // flow, or losing the cascade's wildcard arm linkage),
    // `build_types` would either return `Err` or `Ok(None)` here.
    //
    // Upstream parity reference: `annrpython.py:73-97 build_types`
    // dispatches through `get_call_parameters` → `FunctionDesc.
    // specialize` → `cachedgraph` → `buildgraph` →
    // `translator.buildflowgraph(pyobj)`. Because pyre's
    // `_prebuilt_graphs` short-circuit at `translator.py:50-51`
    // returns the seeded adapter-built graph, `build_flow` never runs
    // here — the full annotation pass operates on the adapter's
    // output.
    let item = parse_item_fn(
        "fn classify(x: i64) -> i64 {
            let val = if x > 0 { x } else { 0 - x };
            match val {
                0 => 0,
                _ => 1,
            }
        }",
    );
    let (t, host) = Translation::from_rust_item_fn(&item).expect("translation");

    let ann = RPythonAnnotator::new_with_translator(
        Some(std::rc::Rc::clone(&t.context)),
        None,
        None,
        false,
    );
    let result = ann
        .build_types(&host, &[AnnotationSpec::Int], true, false)
        .expect("build_types must succeed on M2.5e construct combination");
    match result {
        Some(sv) => {
            let ty = format!("{sv:?}");
            assert!(
                ty.contains("Integer"),
                "expected SomeValue::Integer (the match arms emit constant ints), got {ty}"
            );
        }
        None => panic!(
            "build_types should yield an annotation for a function whose every \
             arm produces an int constant; getting None means the cascade's \
             join-block did not propagate the arm values into the returnblock Link"
        ),
    }
}

#[test]
fn statement_position_if_else_join_has_only_merged_local_inputargs() {
    // Pins the upstream framestate discipline for statement-position
    // `if-else`. Upstream `framestate.py:33` defines
    // `mergeable = locals_w + stack`, and CPython 2.x bytecode for
    // `if cond: body1 else: body2` is `POP_JUMP_IF_FALSE` +
    // body bytecodes + `JUMP_FORWARD` (`flowcontext.py:756`) —
    // neither arm pushes a value onto the stack, so the join's
    // framestate has the same stack depth as the pre-fork state.
    //
    // The corresponding adapter contract: when `lower_if` is called
    // from `lower_block`'s statement arm (`!is_last || semi.is_some()`),
    // the join block's `inputargs` must cover ONLY the merged locals —
    // no extra slot for the if-expression's value.
    //
    // Fixture: a function whose body opens with a statement-position
    // `if-else` (the `;` after `else { }` makes it a `Stmt::Expr` with
    // `semi = Some`), followed by a tail expression that does not
    // depend on the if's value. Each arm emits a real arithmetic op so
    // the branch join remains visible in the graph; the op result is
    // still statement-discarded.
    //
    // The assertion below inspects this fixture's single non-special
    // join directly: it must have exactly two inputargs, the merged
    // locals `c` and `a`.
    let item = parse_item_fn(
        "fn f(c: bool, a: i64) -> i64 {
            if c { a + 1; } else { a - 1; };
            a
        }",
    );
    let graph = build_flow_from_rust(&item).expect("raw adapter graph");
    assert_statement_if_else_join_has_only_merged_local_inputargs(&graph, 2);

    // Cross-check the value-position fixture: a `let val = if ... else ...;`
    // intentionally produces a value-position join with a tail slot.
    // This is a legal shape even if a later version of the fixture stops
    // using `val`, so we assert only that the tail slot exists and can be
    // carried through graph construction.
    let item_value_pos = parse_item_fn(
        "fn g(x: i64) -> i64 {
            let val = if x > 0 { x } else { 0 - x };
            val + 1
        }",
    );
    let graph_value_pos = build_flow_from_rust(&item_value_pos).expect("raw adapter graph");
    assert_value_position_if_else_join_has_tail_inputarg(&graph_value_pos);
}
