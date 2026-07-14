//! Barrett rewrite for wide divisions and modmuls by compile-time constants.
//!
//! LLVM has no fast lowering for `udiv`/`urem` wider than 128 bits on
//! riscv64: it expands them into bit-serial shift-subtract loops (one
//! quotient bit per iteration). When the divisor/modulus is a compile-time
//! constant, the operation can instead be done by multiplication with a
//! precomputed fixed-point reciprocal (Barrett / Granlund-Montgomery).
//!
//! The reduction sequences are ~900 instructions each. Emitting them inline
//! would overflow PolkaVM's basic-block instruction limit whenever a
//! contract chains several modular operations in one straight-line block
//! (e.g. elliptic-curve field arithmetic). So each reduction is **outlined**
//! into a `noinline` helper function, generated once per (operation,
//! constant), and every site becomes a small call. This keeps basic blocks
//! small, deduplicates the code, and matches how the stock stdlib and revm
//! structure the same work (a routine you call, not inlined math).
//!
//! Runs after an `ipsccp` prepass so constants exposed by inlining and
//! interprocedural constant propagation are visible; runtime divisors are
//! left untouched and take the generic path. Each reciprocal is verified
//! against its defining inequality at emission; violation aborts compilation.

use std::num::NonZeroU32;

use inkwell::attributes::AttributeLoc;
use inkwell::module::Linkage;
use inkwell::types::StringRadix;
use inkwell::values::{
    AnyValue, BasicValue, FunctionValue, InstructionOpcode, InstructionValue, IntValue,
};
use num::{BigUint, One, Zero};

/// Minimum bit width handled for plain division. Below 128 bits the target
/// has native or libcall lowerings that are already adequate.
const MIN_WIDTH: u32 = 129;
/// Maximum bit width handled.
const MAX_WIDTH: u32 = 256;
/// Name prefix for the outlined helper functions.
const PREFIX: &str = "__cbarrett_";

/// Rewrites eligible constant-divisor divisions and constant-modulus mulmods
/// in `module` into calls to outlined Barrett helpers. Returns the number of
/// rewritten sites.
pub fn run(module: &inkwell::module::Module) -> usize {
    // Collect all sites before mutating (adding helper functions would
    // otherwise invalidate the function/instruction iterators).
    let mut mulmod_calls = Vec::new();
    let mut div_sites = Vec::new();
    let has_mulmod = module.get_function("__mulmod").is_some();
    for function in module.get_functions() {
        if function
            .get_name()
            .to_str()
            .map(|name| name.starts_with(PREFIX))
            .unwrap_or(false)
        {
            continue; // don't reprocess our own helpers
        }
        for block in function.get_basic_block_iter() {
            let mut instruction = block.get_first_instruction();
            while let Some(current) = instruction {
                instruction = current.get_next_instruction();
                match current.get_opcode() {
                    InstructionOpcode::UDiv | InstructionOpcode::URem => {
                        if let Some(divisor) = eligible_constant_divisor(&current) {
                            div_sites.push((current, divisor));
                        }
                    }
                    InstructionOpcode::Call if has_mulmod => {
                        if let Some(modulus) = eligible_mulmod_call(&current) {
                            mulmod_calls.push((current, modulus));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let mut rewritten = 0;
    for (call, modulus) in mulmod_calls {
        rewrite_mulmod_call(module, &call, &modulus);
        rewritten += 1;
    }
    for (instr, divisor) in div_sites {
        rewrite_division(module, &instr, &divisor);
        rewritten += 1;
    }
    rewritten
}

// ---------------------------------------------------------------------------
// Eligibility
// ---------------------------------------------------------------------------

/// Returns the divisor when `instr` is a division of width
/// [`MIN_WIDTH`]..=[`MAX_WIDTH`] by an eligible integer constant.
fn eligible_constant_divisor(instr: &InstructionValue) -> Option<BigUint> {
    let ty = instr.get_type().into_int_type();
    let width = ty.get_bit_width();
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) {
        return None;
    }
    let divisor = instr.get_operand(1)?.value()?.into_int_value();
    if !divisor.is_const() {
        return None;
    }
    let value = parse_wide_constant(&divisor, width)?;
    if value <= BigUint::one() {
        return None; // 0/1: leave to LLVM's folding
    }
    if (&value & (&value - 1u8)).is_zero() {
        return None; // power of two: already a shift/mask
    }
    Some(value)
}

/// Returns the modulus when `call` is `__mulmod(a, b, C)` with a 256-bit
/// compile-time constant modulus. HAC 14.42 Barrett needs `a*b < 2^(2t)`;
/// since `a, b < 2^256`, this holds exactly at `t = 256`, so only 256-bit
/// moduli qualify. Smaller moduli keep the generic `__mulmod` path.
fn eligible_mulmod_call(call: &InstructionValue) -> Option<BigUint> {
    let count = call.get_num_operands();
    if count != 4 {
        return None; // three arguments plus the callee
    }
    let callee = call.get_operand(count - 1)?.value()?.into_pointer_value();
    if callee.get_name().to_str().ok()? != "__mulmod" {
        return None;
    }
    let modulus = call.get_operand(2)?.value()?.into_int_value();
    if !modulus.is_const() {
        return None;
    }
    let value = parse_wide_constant(&modulus, 256)?;
    if value.bits() as u32 != 256 {
        return None;
    }
    Some(value)
}

/// Extracts an arbitrary-width unsigned constant from an `IntValue`.
fn parse_wide_constant(value: &IntValue, width: u32) -> Option<BigUint> {
    if let Some(small) = value.get_zero_extended_constant() {
        return Some(BigUint::from(small));
    }
    let text = value.print_to_string().to_string();
    let literal = text.split_whitespace().last()?;
    if let Some(magnitude) = literal.strip_prefix('-') {
        let magnitude: BigUint = magnitude.parse().ok()?;
        Some((BigUint::one() << width) - magnitude)
    } else {
        literal.parse().ok()
    }
}

// ---------------------------------------------------------------------------
// Rewrites: replace each site with a call to an outlined helper
// ---------------------------------------------------------------------------

fn rewrite_mulmod_call(
    module: &inkwell::module::Module,
    call: &InstructionValue,
    modulus: &BigUint,
) {
    let helper = mulmod_helper(module, modulus);
    let context = module.get_context();
    let builder = context.create_builder();
    builder.position_before(call);
    let a = call.get_operand(0).unwrap().value().unwrap().into_int_value();
    let b = call.get_operand(1).unwrap().value().unwrap().into_int_value();
    let result = builder
        .build_call(helper, &[a.into(), b.into()], "cbarrett_mm")
        .expect("call")
        .try_as_basic_value()
        .basic()
        .expect("mulmod helper returns a value")
        .into_int_value();
    call.replace_all_uses_with(&result.as_instruction_value().expect("call is an instruction"));
    call.erase_from_basic_block();
}

fn rewrite_division(module: &inkwell::module::Module, instr: &InstructionValue, divisor: &BigUint) {
    let ty = instr.get_type().into_int_type();
    let width = ty.get_bit_width();
    let is_rem = instr.get_opcode() == InstructionOpcode::URem;
    let helper = division_helper(module, divisor, width, is_rem);
    let context = module.get_context();
    let builder = context.create_builder();
    builder.position_before(instr);
    let x = instr.get_operand(0).unwrap().value().unwrap().into_int_value();
    let result = builder
        .build_call(helper, &[x.into()], "cbarrett_div")
        .expect("call")
        .try_as_basic_value()
        .basic()
        .expect("division helper returns a value")
        .into_int_value();
    instr.replace_all_uses_with(&result.as_instruction_value().expect("call is an instruction"));
    instr.erase_from_basic_block();
}

// ---------------------------------------------------------------------------
// Outlined helper generation (get-or-create, deduplicated by name)
// ---------------------------------------------------------------------------

fn dec(value: &BigUint) -> String {
    value.to_str_radix(10)
}

/// `noinline internal i256 @__cbarrett_mulmod_<hex>(i256, i256)` computing
/// `(a*b) mod C` via HAC 14.42 Barrett (bit-shift form) over i576.
fn mulmod_helper<'ctx>(
    module: &inkwell::module::Module<'ctx>,
    modulus: &BigUint,
) -> FunctionValue<'ctx> {
    let name = format!("{PREFIX}mulmod_{}", modulus.to_str_radix(16));
    if let Some(existing) = module.get_function(&name) {
        return existing;
    }
    let context = module.get_context();
    let i256 = int_ty(context, 256);
    let i512 = int_ty(context, 512);
    let i576 = int_ty(context, 576);

    let function = module.add_function(
        &name,
        i256.fn_type(&[i256.into(), i256.into()], false),
        Some(Linkage::Internal),
    );
    mark_noinline(context, function);
    let entry = context.append_basic_block(function, "entry");
    let builder = context.create_builder();
    builder.position_at_end(entry);

    let a = function.get_nth_param(0).unwrap().into_int_value();
    let b = function.get_nth_param(1).unwrap().into_int_value();

    let t = 256u64;
    let two_t = BigUint::one() << (2 * t);
    let mu = &two_t / modulus;
    assert!(
        &mu * modulus <= two_t && (&mu + BigUint::one()) * modulus > two_t,
        "const-division: mulmod reciprocal self-check failed for {modulus}"
    );

    let m512 = i512.const_int_from_string(&dec(modulus), StringRadix::Decimal).unwrap();
    let mu576 = i576.const_int_from_string(&dec(&mu), StringRadix::Decimal).unwrap();

    let aw = builder.build_int_z_extend(a, i512, "aw").unwrap();
    let bw = builder.build_int_z_extend(b, i512, "bw").unwrap();
    let x = builder.build_int_mul(aw, bw, "x").unwrap();
    let pre = builder
        .build_right_shift(x, i512.const_int(t - 1, false), false, "pre")
        .unwrap();
    let pre576 = builder.build_int_z_extend(pre, i576, "pre576").unwrap();
    let prod = builder.build_int_mul(pre576, mu576, "prod").unwrap();
    let q576 = builder
        .build_right_shift(prod, i576.const_int(t + 1, false), false, "q576")
        .unwrap();
    let q = builder.build_int_truncate(q576, i512, "q").unwrap();
    let qm = builder.build_int_mul(q, m512, "qm").unwrap();
    let mut r = builder.build_int_sub(x, qm, "r").unwrap();
    for step in 0..2 {
        r = correct(&builder, r, m512, step);
    }
    let result = builder.build_int_truncate(r, i256, "res").unwrap();
    builder.build_return(Some(&result)).unwrap();
    function
}

/// `noinline internal iW @__cbarrett_{udiv|urem}_<W>_<hex>(iW)` computing
/// `x / C` or `x % C` via Barrett (multiply by `floor(2^W / C)`).
fn division_helper<'ctx>(
    module: &inkwell::module::Module<'ctx>,
    divisor: &BigUint,
    width: u32,
    is_rem: bool,
) -> FunctionValue<'ctx> {
    let op = if is_rem { "urem" } else { "udiv" };
    let name = format!("{PREFIX}{op}_{width}_{}", divisor.to_str_radix(16));
    if let Some(existing) = module.get_function(&name) {
        return existing;
    }
    let context = module.get_context();
    let ty = int_ty(context, width);

    let bound = BigUint::one() << width;
    let mu = &bound / divisor;
    assert!(
        &mu * divisor <= bound && (&mu + BigUint::one()) * divisor > bound,
        "const-division: reciprocal self-check failed for divisor {divisor}"
    );
    let wide_bits = (((width + mu.bits() as u32) + 63) / 64) * 64;
    let wide = int_ty(context, wide_bits);

    let function = module.add_function(
        &name,
        ty.fn_type(&[ty.into()], false),
        Some(Linkage::Internal),
    );
    mark_noinline(context, function);
    let entry = context.append_basic_block(function, "entry");
    let builder = context.create_builder();
    builder.position_at_end(entry);

    let x = function.get_nth_param(0).unwrap().into_int_value();
    let c = ty.const_int_from_string(&dec(divisor), StringRadix::Decimal).unwrap();
    let mu_wide = wide.const_int_from_string(&dec(&mu), StringRadix::Decimal).unwrap();

    let xw = builder.build_int_z_extend(x, wide, "xw").unwrap();
    let prod = builder.build_int_mul(xw, mu_wide, "prod").unwrap();
    let shifted = builder
        .build_right_shift(prod, wide.const_int(width as u64, false), false, "shr")
        .unwrap();
    let mut q = builder.build_int_truncate(shifted, ty, "q").unwrap();
    let qc = builder.build_int_mul(q, c, "qc").unwrap();
    let mut r = builder.build_int_sub(x, qc, "r").unwrap();
    let one = ty.const_int(1, false);
    for step in 0..2 {
        let needs = builder
            .build_int_compare(inkwell::IntPredicate::UGE, r, c, &format!("ge{step}"))
            .unwrap();
        let r_sub = builder.build_int_sub(r, c, &format!("rs{step}")).unwrap();
        let q_add = builder.build_int_add(q, one, &format!("qa{step}")).unwrap();
        r = builder
            .build_select(needs, r_sub, r, &format!("r{step}"))
            .unwrap()
            .into_int_value();
        q = builder
            .build_select(needs, q_add, q, &format!("q{step}"))
            .unwrap()
            .into_int_value();
    }
    builder.build_return(Some(if is_rem { &r } else { &q })).unwrap();
    function
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn int_ty(context: inkwell::context::ContextRef<'_>, bits: u32) -> inkwell::types::IntType<'_> {
    context
        .custom_width_int_type(NonZeroU32::new(bits).expect("nonzero width"))
        .expect("custom width type")
}

fn mark_noinline(context: inkwell::context::ContextRef<'_>, function: FunctionValue<'_>) {
    let kind = inkwell::attributes::Attribute::get_named_enum_kind_id("noinline");
    function.add_attribute(AttributeLoc::Function, context.create_enum_attribute(kind, 0));
}

/// One conditional Barrett correction: `if r >= m { r -= m }`, via select.
fn correct<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
    r: IntValue<'ctx>,
    m: IntValue<'ctx>,
    step: usize,
) -> IntValue<'ctx> {
    let needs = builder
        .build_int_compare(inkwell::IntPredicate::UGE, r, m, &format!("mge{step}"))
        .unwrap();
    let sub = builder.build_int_sub(r, m, &format!("ms{step}")).unwrap();
    builder
        .build_select(needs, sub, r, &format!("mr{step}"))
        .unwrap()
        .into_int_value()
}
