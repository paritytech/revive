//! NEW Yul OptimziR Kit (newyork)
//!
//! This crate provides a custom intermediate representation (IR) for the Revive
//! compiler, positioned between Yul and LLVM IR. It enables domain-specific
//! optimizations that LLVM cannot perform because it lacks semantic knowledge
//! of the PolkaVM target and EVM/Solidity source domains.
//!
//! # Architecture
//!
//! ```text
//! Yul AST ──► newyork IR ──► [Optimizations] ──► LLVM IR ──► RISC-V
//!          (from_yul)      (passes)           (to_llvm)
//! ```
//!
//! # Design Principles
//!
//! 1. **SSA with Structured Control Flow** - Preserves high-level structure from Yul
//! 2. **Explicit Types with Address Spaces** - Every value has a known bit-width
//! 3. **Pure Expressions vs Effectful Statements** - Enables easier reasoning
//! 4. **Semantic Annotations** - Storage/memory operations tagged with region info
//!
//! # Modules
//!
//! - [`ir`] - Core IR data structures (types, values, statements, expressions)
//! - [`ssa`] - SSA builder for variable tracking and phi-node insertion
//! - [`from_yul`] - Translation from Yul AST to newyork IR
//! - [`to_llvm`] - LLVM code generation from newyork IR
//! - [`type_inference`] - Type inference pass for narrowing integer widths
//! - [`heap_opt`] - Heap optimization for partial big-endian emulation
//! - [`mem_opt`] - Memory optimization (load-after-store, dead store elimination)
//! - [`inline`] - Function inlining with custom heuristics for PolkaVM
//!
//! For now, allow missing docs while the crate is in development.
#![allow(missing_docs)]
#![deny(clippy::all)]

/// Environment variable: when set, dumps the newyork IR for every translated object to
/// `/tmp/newyork_ir_<object>.txt` after optimization passes have run.
pub const NEWYORK_DUMP_IR_ENV: &str = "NEWYORK_DUMP_IR";

pub mod compound_outlining;
pub mod from_yul;
pub mod guard_narrow;
pub mod heap_opt;
pub mod inline;
pub mod ir;
pub mod mem_opt;
pub mod printer;
pub mod simplify;
pub mod ssa;
pub mod to_llvm;
pub mod type_inference;
pub mod validate;

pub use from_yul::{TranslationError, YulTranslator};
pub use heap_opt::{
    AccessPattern, HeapAnalysis, HeapAnalysisStats, HeapOptResults, MemorySlot, OffsetInfo,
};
pub use inline::{
    analyze_call_graph, inline_functions, CallGraphAnalysis, InlineDecision, InlineResults,
};
pub use ir::{
    AddressSpace, BinaryOperation, BitWidth, Block, CallKind, CreateKind, Expression, Function,
    FunctionId, MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value,
    ValueId,
};
pub use mem_opt::{MemOptResults, MemoryOptimizer};
pub use printer::{
    print_expression, print_function, print_object, print_statement, Printer, PrinterConfig,
};
pub use simplify::{
    deduplicate_functions, deduplicate_functions_fuzzy, fold_constant_keccak, Simplifier,
    SimplifyResults,
};
pub use ssa::SsaBuilder;
pub use to_llvm::{CodegenError, LlvmCodegen};
pub use type_inference::{TypeConstraint, TypeInference};
pub use validate::{validate_function, validate_object, ValidationError, ValidationResult};
/// Maximum number of param/return narrowing iterations.
///
/// Each iteration narrows function signatures, re-runs full type inference with the new widths so
/// narrowed parameters cascade through `add`/`and`/etc. forward, and then refines call-site
/// demands. Four iterations is enough to reach a fixed point on the OZ corpus; any further work is
/// bounded by an explicit `changed` check.
const PARAM_NARROW_ITERATIONS: u32 = 4;

/// Per-call-site overhead in IR units used by [`inline_by_shrink_prediction`]. Roughly maps to the
/// LLVM cost of a `tail call` + per-arg setup. `make_inline_decisions` treats `1 IR unit ≈ 1 LLVM
/// byte` empirically, so a small constant here is correct.
const SHRINK_PREDICTION_CALL_OVERHEAD: usize = 2;

/// Minimum body size considered by [`inline_by_shrink_prediction`]. Below the always-inline
/// threshold the existing inliner already inlines, so shrink-prediction adds no information.
const SHRINK_PREDICTION_MIN_CANDIDATE_SIZE: usize = 7;

/// Maximum body size considered by [`inline_by_shrink_prediction`]. Beyond this, body-side
/// simplifier folding cannot realistically halve a 70+ statement function via a handful of literal
/// args, so prediction is fragile and disabled.
const SHRINK_PREDICTION_MAX_CANDIDATE_SIZE: usize = 60;

/// Result of translating a Yul object to newyork IR.
pub struct TranslationResult {
    /// The translated IR object.
    pub object: Object,
    /// Heap optimization results (identifies where byte-swapping can be skipped).
    pub heap_opt: HeapOptResults,
    /// Type inference results (narrower types for values).
    pub type_info: TypeInference,
    /// Memory optimization results (load-after-store, dead store elimination).
    pub mem_opt: MemOptResults,
    /// Inlining results (which functions were inlined and removed).
    pub inline_results: InlineResults,
}

/// Translates a Yul object to newyork IR.
///
/// This is the main entry point for converting Yul AST to the new IR format.
/// Returns the IR object along with heap optimization analysis results.
///
/// # Example
///
/// ```ignore
/// use revive_newyork::translate_yul_object;
/// use revive_yul::parser::statement::object::Object;
///
/// let yul_object: Object = /* parse yul */;
/// let result = translate_yul_object(&yul_object)?;
/// let ir_object = result.object;
/// let heap_opt = result.heap_opt;
/// ```
pub fn translate_yul_object(
    yul_object: &revive_yul::parser::statement::object::Object,
) -> Result<TranslationResult, TranslationError> {
    let mut translator = YulTranslator::new();
    let mut ir_object = translator.translate_object(yul_object)?;

    let (mut inline_results, mem_opt_results) = optimize_object_tree(&mut ir_object);

    if std::env::var(NEWYORK_DUMP_IR_ENV).is_ok() {
        use std::io::Write;
        let dump_path = format!("/tmp/newyork_ir_{}.txt", ir_object.name.replace('/', "_"));
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&dump_path)
        {
            let _ = write!(f, "{}", print_object(&ir_object));
        }
    }

    let mut heap_analysis = HeapAnalysis::new();
    heap_analysis.analyze_object(&ir_object);
    let heap_opt = HeapOptResults::from_analysis(&heap_analysis);

    let mut type_info = TypeInference::new();
    type_info.infer_object_tree(&ir_object);

    let type_info = narrow_signatures_to_fixed_point(&mut ir_object, type_info);

    // Late inline pass: now that parameter narrowing has propagated through the IR and
    // simplification has folded any newly exposed constants, some wrapper functions have shrunk
    // below the inline thresholds. Re-estimate function sizes and run the inliner again to catch
    // these, followed by simplify + dedup to clean up the inlined bodies. This is intentionally
    // separate from the early inliner: the early pass exposes intra-procedural opportunities that
    // drive the rest of the pipeline; the late pass collects the per-function shrinkage produced
    // by every subsequent optimization (mem_opt, compound_outlining, guard_narrow, full type
    // narrowing).
    let type_info = run_late_inline_loop(&mut ir_object, &mut inline_results, type_info);

    if let Err(errors) = validate::validate_object(&ir_object) {
        let details = errors
            .iter()
            .map(|error| format!("  - {error}"))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "ICE: IR validation failed for object `{}` after optimization pipeline:\n{}",
            ir_object.name, details,
        );
    }

    Ok(TranslationResult {
        object: ir_object,
        heap_opt,
        type_info,
        mem_opt: mem_opt_results,
        inline_results,
    })
}

/// Maximum number of late inline + simplify + narrow iterations.
///
/// One round captures every additional inline the cost model can make once the pipeline's other
/// passes have settled (measured on the OZ corpus — two or more rounds produce identical output).
/// The bound is kept as a constant so future cost-model work that depends on cascading shrinkage
/// across iterations doesn't have to re-introduce the scaffolding.
const LATE_INLINE_ITERATIONS: u32 = 1;

/// Runs a fixed-point loop of late inline + simplify + dedup + type narrow.
///
/// After the early inline + heap + mem_opt + compound_outlining + guard_narrow + first round of
/// param narrowing has completed, many wrapper helpers have collapsed to a handful of statements.
/// The early inliner couldn't act on them because they were still wrapped in pre-simplify noise;
/// running the inliner again at this point — with re-estimated function sizes and on top of the
/// narrowed signatures — picks up the residue.
///
/// The function takes ownership of `type_info` so we can reseed it after the inlined IR is in
/// place; the returned `TypeInference` reflects the final state used for downstream codegen.
fn run_late_inline_loop(
    ir_object: &mut ir::Object,
    inline_results: &mut InlineResults,
    mut type_info: TypeInference,
) -> TypeInference {
    for _ in 0..LATE_INLINE_ITERATIONS {
        inline::estimate_function_sizes(ir_object);
        let late_results = inline_functions(ir_object);
        inline_results.inlined_call_sites += late_results.inlined_call_sites;
        inline_results
            .removed_functions
            .extend(late_results.removed_functions);
        inline_results.decisions.extend(late_results.decisions);

        let mut simplifier = Simplifier::new();
        simplifier.simplify_object(ir_object);

        // Re-run compound outlining + guard narrowing after inlining: the
        // inlined bodies expose new keccak256_pair+sload pairs and overflow
        // checks that the early pass couldn't see across the call boundary.
        compound_outlining::outline_compounds_in_object(ir_object);
        guard_narrow::narrow_guards_in_object(ir_object);
        let mut simplifier_post = Simplifier::new();
        simplifier_post.simplify_object(ir_object);

        let _ = deduplicate_functions(ir_object);
        let _ = simplify::deduplicate_functions_fuzzy(ir_object);

        type_info = TypeInference::new();
        type_info.infer_object_tree(ir_object);
        type_info = narrow_signatures_to_fixed_point(ir_object, type_info);
    }
    // Final size refresh so LLVM-level inline hints (set during codegen) see
    // the post-simplify, post-narrow shape rather than the larger pre-cleanup
    // estimates that `inline_functions` last set internally.
    inline::estimate_function_sizes(ir_object);
    type_info
}

/// Iteratively narrows function parameter and return types until no change.
///
/// Each iteration applies four narrowing strategies — forward param, caller-driven
/// param, forward return, demand-driven return — and re-runs full type inference
/// so the new signature widths cascade through every function body before the next
/// iteration. Bounded by [`PARAM_NARROW_ITERATIONS`] but exits early on a fixed point.
fn narrow_signatures_to_fixed_point(
    ir_object: &mut ir::Object,
    mut type_info: TypeInference,
) -> TypeInference {
    for _ in 0..PARAM_NARROW_ITERATIONS {
        let mut changed = type_info.narrow_function_params(ir_object);
        changed |= type_info.narrow_function_params_from_callers(ir_object);
        changed |= type_info.narrow_function_returns(ir_object);
        changed |= type_info.narrow_function_returns_from_demand(ir_object);
        if !changed {
            break;
        }
        type_info = TypeInference::new();
        type_info.infer_object_tree(ir_object);
        type_info.refine_demands_from_params(ir_object);
    }
    type_info
}

/// Runs the full newyork optimization pipeline on an object and its subobjects.
///
/// Pass order matters: inlining runs first to expose intra-procedural opportunities,
/// then simplify+dedup clean up the IR, then mem_opt + FMP propagation expose
/// constant keccak inputs, and finally compound outlining and guard narrowing
/// rewrite specialized patterns. Each of those rewriting passes can produce new
/// constants and dead code, so a simplify pass follows each cluster; a second dedup
/// catches near-duplicates that only emerge after canonicalization.
///
/// Subobjects are processed recursively and their inline/memory results are merged
/// into the returned aggregates.
fn optimize_object_tree(object: &mut ir::Object) -> (InlineResults, MemOptResults) {
    let mut inline_results = inline_functions(object);

    let mut simplifier = Simplifier::new();
    let _simplify_stats = simplifier.simplify_object(object);

    let _dedup_count = deduplicate_functions(object);
    let _fuzzy_dedup_count = simplify::deduplicate_functions_fuzzy(object);

    let mut mem_optimizer = MemoryOptimizer::new();
    let mut mem_opt_results = mem_optimizer.optimize_object(object);
    let mut fmp_prop = mem_opt::FmpPropagation::new(0);
    fmp_prop.propagate_object(object);
    mem_opt_results.fmp_loads_eliminated += fmp_prop.loads_eliminated;

    simplify::fold_constant_keccak(object);

    let mut simplifier2 = Simplifier::new();
    simplifier2.simplify_object(object);

    compound_outlining::outline_compounds_in_object(object);
    guard_narrow::narrow_guards_in_object(object);

    let mut simplifier3 = Simplifier::new();
    simplifier3.simplify_object(object);

    let _eliminated = eliminate_constant_parameters(object);
    if _eliminated > 0 {
        let mut simplifier_post_const = Simplifier::new();
        simplifier_post_const.simplify_object(object);
    }

    let _predicted = inline_by_shrink_prediction(object);
    if _predicted > 0 {
        let mut simplifier_post_predict = Simplifier::new();
        simplifier_post_predict.simplify_object(object);
    }

    let _dedup_count2 = deduplicate_functions(object);
    let _fuzzy_dedup_count2 = simplify::deduplicate_functions_fuzzy(object);

    for subobject in &mut object.subobjects {
        let (sub_inline, sub_mem_opt) = optimize_object_tree(subobject);
        inline_results.inlined_call_sites += sub_inline.inlined_call_sites;
        inline_results
            .removed_functions
            .extend(sub_inline.removed_functions);
        inline_results.decisions.extend(sub_inline.decisions);
        mem_opt_results.loads_eliminated += sub_mem_opt.loads_eliminated;
        mem_opt_results.stores_eliminated += sub_mem_opt.stores_eliminated;
        mem_opt_results.values_tracked += sub_mem_opt.values_tracked;
        mem_opt_results.keccak_pairs_fused += sub_mem_opt.keccak_pairs_fused;
        mem_opt_results.keccak_singles_fused += sub_mem_opt.keccak_singles_fused;
        mem_opt_results.fmp_loads_eliminated += sub_mem_opt.fmp_loads_eliminated;
    }

    (inline_results, mem_opt_results)
}

/// Eliminate parameters whose argument is the same compile-time literal at
/// every call site. The parameter is removed from the function signature, an
/// equivalent `Let` binding is prepended to the body so existing uses of the
/// parameter's `ValueId` keep working, and the matching argument is dropped
/// from every call expression.
///
/// Returns the total number of `(function, parameter)` pairs eliminated
/// (including those inside subobjects).
///
/// This is an IR-level analogue of LLVM's `deadargelim` + `ipsccp` constant
/// propagation through call boundaries — necessary because most newyork
/// helpers carry `noinline` for code-size reasons, which blocks LLVM's IPO
/// from baking constants into the function bodies.
fn eliminate_constant_parameters(object: &mut ir::Object) -> usize {
    use std::collections::BTreeMap;

    let mut total = 0;
    for subobject in &mut object.subobjects {
        total += eliminate_constant_parameters(subobject);
    }

    let mut block_literals: BTreeMap<u32, (num::BigUint, Type)> = BTreeMap::new();
    collect_top_level_literals(&object.code.statements, &mut block_literals);

    #[derive(Clone)]
    enum ArgState {
        Initial,
        Constant(num::BigUint, Type),
        Variable,
    }

    let mut arg_states: BTreeMap<(FunctionId, usize), ArgState> = BTreeMap::new();
    let mut call_counts: BTreeMap<FunctionId, usize> = BTreeMap::new();
    for &function_id in object.functions.keys() {
        call_counts.insert(function_id, 0);
    }

    let mut record_call =
        |function_id: FunctionId,
         arguments: &[Value],
         extra_literals: &BTreeMap<u32, (num::BigUint, Type)>| {
            *call_counts.entry(function_id).or_insert(0) += 1;
            for (parameter_index, argument) in arguments.iter().enumerate() {
                let new_state = match extra_literals
                    .get(&argument.id.0)
                    .or_else(|| block_literals.get(&argument.id.0))
                {
                    Some((value, value_type)) => ArgState::Constant(value.clone(), *value_type),
                    None => ArgState::Variable,
                };
                let entry = arg_states
                    .entry((function_id, parameter_index))
                    .or_insert(ArgState::Initial);
                let merged = match (entry.clone(), new_state) {
                    (ArgState::Initial, fresh) => fresh,
                    (ArgState::Variable, _) | (_, ArgState::Variable) => ArgState::Variable,
                    (ArgState::Constant(value_a, type_a), ArgState::Constant(value_b, type_b))
                        if value_a == value_b && type_a == type_b =>
                    {
                        ArgState::Constant(value_a, type_a)
                    }
                    _ => ArgState::Variable,
                };
                *entry = merged;
            }
        };

    visit_calls(&object.code.statements, &mut record_call);
    for function in object.functions.values() {
        visit_calls(&function.body.statements, &mut record_call);
    }

    let mut by_function: BTreeMap<FunctionId, Vec<(usize, num::BigUint, Type)>> = BTreeMap::new();
    for ((function_id, parameter_index), state) in arg_states {
        if call_counts.get(&function_id).copied().unwrap_or(0) < 1 {
            continue;
        }
        if let ArgState::Constant(value, value_type) = state {
            by_function
                .entry(function_id)
                .or_default()
                .push((parameter_index, value, value_type));
        }
    }

    if by_function.is_empty() {
        return total;
    }

    let mut drop_indices: BTreeMap<FunctionId, Vec<usize>> = BTreeMap::new();

    for (function_id, mut entries) in by_function {
        let Some(function) = object.functions.get_mut(&function_id) else {
            continue;
        };

        // Don't eliminate every parameter — leave the function with at least
        // one input slot so the call-site arity matches non-zero. (Yul allows
        // zero-arg functions, so this isn't strictly necessary, but it keeps
        // the change conservative and the diff smaller.)
        if entries.len() >= function.parameters.len() {
            entries.truncate(function.parameters.len().saturating_sub(1));
            if entries.is_empty() {
                continue;
            }
        }

        entries.sort_by_key(|entry| entry.0);
        let mut prologue: Vec<Statement> = Vec::with_capacity(entries.len());
        for (parameter_index, value, value_type) in &entries {
            let (parameter_id, _) = function.parameters[*parameter_index];
            prologue.push(Statement::Let {
                bindings: vec![parameter_id],
                value: Expression::Literal {
                    value: value.clone(),
                    value_type: *value_type,
                },
            });
        }

        let dropped: Vec<usize> = entries.iter().map(|(index, _, _)| *index).collect();
        let mut dropped_descending = dropped.clone();
        dropped_descending.sort_by(|a, b| b.cmp(a));
        for parameter_index in dropped_descending {
            function.parameters.remove(parameter_index);
        }

        let mut new_body = prologue;
        new_body.extend(std::mem::take(&mut function.body.statements));
        function.body.statements = new_body;
        function.size_estimate = inline::estimate_block_size(&function.body);

        total += entries.len();
        drop_indices.insert(function_id, dropped);
    }

    if drop_indices.is_empty() {
        return total;
    }

    trim_call_arguments(&mut object.code.statements, &drop_indices);
    for function in object.functions.values_mut() {
        trim_call_arguments(&mut function.body.statements, &drop_indices);
    }

    total
}

/// Inlines functions whose predicted post-substitution size (with
/// callsite literal arguments propagated through the body and a fresh
/// simplifier pass run on the result) beats the cost of keeping them.
///
/// For each function in the cost-benefit range:
/// 1. Walk every call site, build the literal-arg map for that site.
/// 2. Clone the function body, prepend `Let p_i = Literal(value)` for each
///    literal arg, drop those params from the signature.
/// 3. Run the full simplifier on the cloned function in isolation.
/// 4. Measure the resulting `size_estimate`.
/// 5. Sum the predicted sizes over all call sites.
/// 6. If the sum (no body, no call overhead) beats the keep-cost
///    (body + N × call-overhead), force-inline by lowering
///    `size_estimate` so the next `inline_functions` pass picks the
///    function as `AlwaysInline`.
///
/// Returns the number of functions force-inlined.
fn inline_by_shrink_prediction(object: &mut ir::Object) -> usize {
    use std::collections::BTreeMap;

    let mut total = 0;
    for subobject in &mut object.subobjects {
        total += inline_by_shrink_prediction(subobject);
    }

    let mut block_literals: BTreeMap<u32, (num::BigUint, Type)> = BTreeMap::new();
    collect_top_level_literals(&object.code.statements, &mut block_literals);

    // For each (function, callsite) → ordered list of (param_idx → literal)
    // for the args known at the call site.
    type LiteralArgs = BTreeMap<usize, (num::BigUint, Type)>;
    let mut sites_per_callee: BTreeMap<FunctionId, Vec<LiteralArgs>> = BTreeMap::new();

    let mut record_call =
        |function_id: FunctionId,
         arguments: &[Value],
         extra_literals: &BTreeMap<u32, (num::BigUint, Type)>| {
            let mut site_literals: LiteralArgs = BTreeMap::new();
            for (parameter_index, argument) in arguments.iter().enumerate() {
                if let Some((value, value_type)) = extra_literals
                    .get(&argument.id.0)
                    .or_else(|| block_literals.get(&argument.id.0))
                {
                    site_literals.insert(parameter_index, (value.clone(), *value_type));
                }
            }
            sites_per_callee
                .entry(function_id)
                .or_default()
                .push(site_literals);
        };

    visit_calls(&object.code.statements, &mut record_call);
    for function in object.functions.values() {
        visit_calls(&function.body.statements, &mut record_call);
    }

    let mut force_inline: Vec<FunctionId> = Vec::new();
    for (function_id, sites) in &sites_per_callee {
        if sites.len() < 2 {
            continue;
        }
        let Some(function) = object.functions.get(function_id) else {
            continue;
        };
        if function.size_estimate < SHRINK_PREDICTION_MIN_CANDIDATE_SIZE {
            continue;
        }
        if function.size_estimate > SHRINK_PREDICTION_MAX_CANDIDATE_SIZE {
            continue;
        }
        if !sites.iter().any(|literals| !literals.is_empty()) {
            continue;
        }

        let mut total_predicted = 0;
        for site_literals in sites {
            total_predicted += predict_simplified_size(function, site_literals);
        }
        let keep_cost = function.size_estimate + sites.len() * SHRINK_PREDICTION_CALL_OVERHEAD;
        // Require predicted <= 75% of keep cost. Empirically the
        // 1-IR-unit-≈-1-LLVM-byte assumption is tight, so small predicted
        // wins are noise; 60% changes nothing on the OZ corpus, 80% admits
        // +868 bytes of regression.
        if total_predicted * 4 < keep_cost * 3 {
            force_inline.push(*function_id);
        }
    }

    if force_inline.is_empty() {
        return total;
    }

    // Lower size_estimate below ALWAYS_INLINE_SIZE_THRESHOLD so the next
    // `inline_functions` pass picks these as AlwaysInline, reusing the
    // canonical inliner rather than duplicating its logic.
    for function_id in &force_inline {
        if let Some(function) = object.functions.get_mut(function_id) {
            function.size_estimate = 1;
        }
    }
    let _ = inline_functions(object);
    total += force_inline.len();

    total
}

/// Clones `function`, prepends `Let p_i = Literal(value)` for every
/// `(param_idx, value)` in `literal_args`, drops those parameters from
/// the signature, then runs the full simplifier on the clone in
/// isolation. Returns the post-simplify size estimate.
fn predict_simplified_size(
    function: &ir::Function,
    literal_args: &std::collections::BTreeMap<usize, (num::BigUint, Type)>,
) -> usize {
    let mut temp = function.clone();
    let mut sorted_args: Vec<(usize, num::BigUint, Type)> = literal_args
        .iter()
        .map(|(&idx, (value, value_type))| (idx, value.clone(), *value_type))
        .collect();
    sorted_args.sort_by_key(|entry| entry.0);

    let mut prologue: Vec<Statement> = Vec::with_capacity(sorted_args.len());
    for (idx, value, value_type) in &sorted_args {
        if *idx >= temp.parameters.len() {
            continue;
        }
        let (param_id, _) = temp.parameters[*idx];
        prologue.push(Statement::Let {
            bindings: vec![param_id],
            value: Expression::Literal {
                value: value.clone(),
                value_type: *value_type,
            },
        });
    }
    let mut dropped: Vec<usize> = sorted_args
        .iter()
        .filter(|(idx, _, _)| *idx < temp.parameters.len())
        .map(|(idx, _, _)| *idx)
        .collect();
    dropped.sort_by(|a, b| b.cmp(a));
    for idx in dropped {
        temp.parameters.remove(idx);
    }

    let mut new_body = prologue;
    new_body.extend(std::mem::take(&mut temp.body.statements));
    temp.body.statements = new_body;

    let original_id = temp.id;
    let mut tmp_object = ir::Object::new("tmp_shrink_prediction".to_string());
    tmp_object.functions.insert(original_id, temp);
    let mut simplifier = Simplifier::new();
    simplifier.simplify_object(&mut tmp_object);

    let simplified_function = match tmp_object.functions.get(&original_id) {
        Some(function) => function,
        None => return function.size_estimate,
    };
    inline::estimate_block_size(&simplified_function.body)
}

/// Records `Let id = Literal v` bindings that occur at the top level of a
/// statement list (not inside any nested branch).
fn collect_top_level_literals(
    statements: &[Statement],
    literals: &mut std::collections::BTreeMap<u32, (num::BigUint, Type)>,
) {
    for statement in statements {
        if let Statement::Let {
            bindings,
            value: Expression::Literal { value, value_type },
        } = statement
        {
            if bindings.len() == 1 {
                literals.insert(bindings[0].0, (value.clone(), *value_type));
            }
        }
    }
}

/// Visits every Call expression in a statement list (recursing into nested
/// regions) and invokes `record(function_id, arguments, branch_literals)`.
/// `branch_literals` covers `Let id = Literal v` bindings encountered earlier
/// in the same statement list — branches inherit the literals from the
/// containing block but their own literals stay local to that branch.
fn visit_calls<F>(statements: &[Statement], record: &mut F)
where
    F: FnMut(FunctionId, &[Value], &std::collections::BTreeMap<u32, (num::BigUint, Type)>),
{
    use std::collections::BTreeMap;
    let mut literals: BTreeMap<u32, (num::BigUint, Type)> = BTreeMap::new();
    visit_calls_inner(statements, &mut literals, record);
}

fn visit_calls_inner<F>(
    statements: &[Statement],
    literals: &mut std::collections::BTreeMap<u32, (num::BigUint, Type)>,
    record: &mut F,
) where
    F: FnMut(FunctionId, &[Value], &std::collections::BTreeMap<u32, (num::BigUint, Type)>),
{
    for statement in statements {
        match statement {
            Statement::Let { bindings, value } => {
                if let Expression::Literal { value, value_type } = value {
                    if bindings.len() == 1 {
                        literals.insert(bindings[0].0, (value.clone(), *value_type));
                    }
                }
                if let Expression::Call {
                    function,
                    arguments,
                } = value
                {
                    record(*function, arguments, literals);
                }
            }
            Statement::Expression(Expression::Call {
                function,
                arguments,
            }) => {
                record(*function, arguments, literals);
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                let saved = literals.clone();
                visit_calls_inner(&then_region.statements, literals, record);
                if let Some(region) = else_region {
                    *literals = saved.clone();
                    visit_calls_inner(&region.statements, literals, record);
                }
                *literals = saved;
            }
            Statement::Switch { cases, default, .. } => {
                let saved = literals.clone();
                for case in cases {
                    visit_calls_inner(&case.body.statements, literals, record);
                    *literals = saved.clone();
                }
                if let Some(region) = default {
                    visit_calls_inner(&region.statements, literals, record);
                }
                *literals = saved;
            }
            Statement::For {
                condition_statements,
                body,
                post,
                ..
            } => {
                let saved = literals.clone();
                visit_calls_inner(condition_statements, literals, record);
                visit_calls_inner(&body.statements, literals, record);
                visit_calls_inner(&post.statements, literals, record);
                *literals = saved;
            }
            Statement::Block(region) => {
                let saved = literals.clone();
                visit_calls_inner(&region.statements, literals, record);
                *literals = saved;
            }
            _ => {}
        }
    }
}

/// Trim arguments from every Call expression in the statement list whose
/// callee appears in `drops`. `drops[fid]` is the ascending list of argument
/// indices to remove.
fn trim_call_arguments(
    statements: &mut [Statement],
    drops: &std::collections::BTreeMap<FunctionId, Vec<usize>>,
) {
    for statement in statements.iter_mut() {
        match statement {
            Statement::Let {
                value:
                    Expression::Call {
                        function,
                        arguments,
                    },
                ..
            } => {
                if let Some(indices) = drops.get(function) {
                    trim_indices(arguments, indices);
                }
            }
            Statement::Expression(Expression::Call {
                function,
                arguments,
            }) => {
                if let Some(indices) = drops.get(function) {
                    trim_indices(arguments, indices);
                }
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                trim_call_arguments(&mut then_region.statements, drops);
                if let Some(region) = else_region {
                    trim_call_arguments(&mut region.statements, drops);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    trim_call_arguments(&mut case.body.statements, drops);
                }
                if let Some(region) = default {
                    trim_call_arguments(&mut region.statements, drops);
                }
            }
            Statement::For {
                condition_statements,
                body,
                post,
                ..
            } => {
                trim_call_arguments(condition_statements, drops);
                trim_call_arguments(&mut body.statements, drops);
                trim_call_arguments(&mut post.statements, drops);
            }
            Statement::Block(region) => {
                trim_call_arguments(&mut region.statements, drops);
            }
            _ => {}
        }
    }
}

fn trim_indices<T: Clone>(values: &mut Vec<T>, indices_ascending: &[usize]) {
    let mut keep: Vec<T> =
        Vec::with_capacity(values.len() - indices_ascending.len().min(values.len()));
    let drop_set: std::collections::BTreeSet<usize> = indices_ascending.iter().copied().collect();
    for (i, v) in values.iter().enumerate() {
        if !drop_set.contains(&i) {
            keep.push(v.clone());
        }
    }
    *values = keep;
}
