//! Barrett rewrite for wide divisions by compile-time constants.
//!
//! LLVM has no fast lowering for `udiv`/`urem` wider than 128 bits on
//! riscv64: its fallback expands them into bit-serial shift-subtract loops
//! (one quotient bit per iteration). When the divisor is a compile-time
//! constant, division can instead be performed by multiplication with a
//! precomputed fixed-point reciprocal (Barrett / Granlund-Montgomery):
//!
//!   mu = floor(2^256 / C)                      (computed here, at compile time)
//!   q0 = (zext(x, 512) * mu) >> 256            (one wide multiply)
//!   r0 = x - q0 * C
//!   two conditional corrections (provably sufficient: the estimate
//!   error is at most 1 for any x < 2^256; see below)
//!
//! Runs after the optimization pipeline, so constants exposed by inlining
//! and interprocedural constant propagation (e.g. Solidity `constant`
//! moduli routed through solc accessor functions) are visible. Divisions
//! by runtime values are left untouched and take the generic path.
//!
//! Error bound: with mu = floor(2^256/C) we have mu > 2^256/C - 1, so for
//! any x < 2^256: x*mu/2^256 > x/C - x/2^256 > x/C - 1, hence
//! q0 = floor(x*mu/2^256) >= floor(x/C) - 1. Since also q0 <= floor(x/C),
//! one correction suffices; two are emitted as margin. Each computed mu is
//! additionally self-verified against its defining inequality at emission
//! time; violation aborts compilation rather than emitting wrong code.

use std::num::NonZeroU32;

use inkwell::types::StringRadix;
use inkwell::values::{AnyValue, BasicValue, InstructionOpcode, InstructionValue, IntValue};
use num::{BigUint, One, Zero};

/// Minimum bit width handled. Below 128 bits the target has native or
/// libcall lowerings that are already adequate.
const MIN_WIDTH: u32 = 129;
/// Maximum bit width handled (the EVM word).
const MAX_WIDTH: u32 = 512;

/// Rewrites eligible constant-divisor divisions in `module`.
/// Returns the number of rewritten instructions.
pub fn run(module: &inkwell::module::Module) -> usize {
    let mut rewritten = rewrite_mulmod_calls(module);
    for function in module.get_functions() {
        let mut candidates = Vec::new();
        for block in function.get_basic_block_iter() {
            let mut instruction = block.get_first_instruction();
            while let Some(current) = instruction {
                instruction = current.get_next_instruction();
                if matches!(
                    current.get_opcode(),
                    InstructionOpcode::UDiv | InstructionOpcode::URem
                ) {
                    candidates.push(current);
                }
            }
        }
        for instr in candidates {
            if let Some(divisor) = eligible_constant_divisor(&instr) {
                rewrite(&instr, &divisor);
                rewritten += 1;
            }
        }
    }
    rewritten
}

/// Returns the divisor value when `instr` is a division of width
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
        return None; // division by zero or one: leave to LLVM's folding
    }
    if (&value & (&value - 1u8)).is_zero() {
        return None; // power of two: LLVM lowers to shift/mask already
    }
    Some(value)
}

/// Extracts an arbitrary-width unsigned constant from an `IntValue`.
///
/// inkwell exposes constants only up to 64 bits directly, so wider values
/// are recovered from the textual form (`iN <signed decimal>`).
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

/// Replaces `instr` (udiv/urem of width w by constant C) with the Barrett
/// multiply-shift-subtract sequence.
fn rewrite(instr: &InstructionValue, divisor: &BigUint) {
    let opcode = instr.get_opcode();
    let block = instr
        .get_parent()
        .expect("division instruction must be in a basic block");
    let context = block.get_context();
    let ty = instr.get_type().into_int_type();
    let width = ty.get_bit_width();
    // mu = floor(2^w / C), self-verified: mu*C <= 2^w < (mu+1)*C.
    let bound = BigUint::one() << width;
    let mu = &bound / divisor;
    // The product x*mu needs at most width + bitlen(mu) bits; size the wide
    // type to that (limb-aligned) rather than 2*width, so the multiply's
    // legalized expansion stays small enough for the PVM basic-block limit.
    let wide_bits = (((width + mu.bits() as u32) + 63) / 64) * 64;
    let wide = context
        .custom_width_int_type(NonZeroU32::new(wide_bits).expect("nonzero width"))
        .expect("custom width type");
    assert!(
        &mu * divisor <= bound && (&mu + BigUint::one()) * divisor > bound,
        "const-division: reciprocal self-check failed for divisor {divisor}"
    );
    assert!(mu < bound, "const-division: reciprocal exceeds width");

    let builder = context.create_builder();
    builder.position_before(instr);

    let dividend = instr
        .get_operand(0)
        .expect("division has a dividend")
        .value()
        .expect("dividend is a value")
        .into_int_value();

    let dec = |v: &BigUint| v.to_str_radix(10);
    let c_narrow = ty
        .const_int_from_string(&dec(divisor), StringRadix::Decimal)
        .expect("divisor literal");
    let mu_wide = wide
        .const_int_from_string(&dec(&mu), StringRadix::Decimal)
        .expect("reciprocal literal");
    let shift_amount = wide.const_int(width as u64, false);
    let one = ty.const_int(1, false);

    let x_wide = builder
        .build_int_z_extend(dividend, wide, "cdiv_x")
        .expect("zext");
    let product = builder
        .build_int_mul(x_wide, mu_wide, "cdiv_mul")
        .expect("mul");
    let shifted = builder
        .build_right_shift(product, shift_amount, false, "cdiv_shr")
        .expect("shr");
    let mut quotient = builder
        .build_int_truncate(shifted, ty, "cdiv_q")
        .expect("trunc");
    let estimate_times_c = builder
        .build_int_mul(quotient, c_narrow, "cdiv_qc")
        .expect("mul");
    let mut remainder = builder
        .build_int_sub(dividend, estimate_times_c, "cdiv_r")
        .expect("sub");

    // Two conditional corrections (error bound is 1; the second is margin).
    for step in 0..2 {
        let needs = builder
            .build_int_compare(
                inkwell::IntPredicate::UGE,
                remainder,
                c_narrow,
                &format!("cdiv_ge{step}"),
            )
            .expect("cmp");
        let r_sub = builder
            .build_int_sub(remainder, c_narrow, &format!("cdiv_rs{step}"))
            .expect("sub");
        let q_add = builder
            .build_int_add(quotient, one, &format!("cdiv_qa{step}"))
            .expect("add");
        remainder = builder
            .build_select(needs, r_sub, remainder, &format!("cdiv_r{step}"))
            .expect("select")
            .into_int_value();
        quotient = builder
            .build_select(needs, q_add, quotient, &format!("cdiv_q{step}"))
            .expect("select")
            .into_int_value();
    }

    let result = match opcode {
        InstructionOpcode::URem => remainder,
        InstructionOpcode::UDiv => quotient,
        _ => unreachable!("only udiv/urem are collected"),
    };
    let replacement = result
        .as_instruction_value()
        .expect("select produces an instruction");
    instr.replace_all_uses_with(&replacement);
    instr.erase_from_basic_block();
}

/// Specializes `__mulmod(a, b, m)` with a compile-time constant modulus into
/// `(a * b) mod m` computed as a 512-bit product followed by a single
/// `urem i512`. No operand pre-reduction is needed — `a, b < 2^256` so the
/// product is `< 2^512` and the reduction handles the full range. The `urem`
/// is Barretted by [`run`] (hence [`MAX_WIDTH`] covers 512). A runtime
/// modulus keeps the `__mulmod` call and the generic path.
fn rewrite_mulmod_calls(module: &inkwell::module::Module) -> usize {
    const MULMOD: &str = "__mulmod";
    if module.get_function(MULMOD).is_none() {
        return 0;
    }
    let context = module.get_context();
    let i256 = context
        .custom_width_int_type(NonZeroU32::new(256).expect("nonzero"))
        .expect("i256");
    let i512 = context
        .custom_width_int_type(NonZeroU32::new(512).expect("nonzero"))
        .expect("i512");

    let mut rewritten = 0;
    for function in module.get_functions() {
        let mut calls = Vec::new();
        for block in function.get_basic_block_iter() {
            let mut instruction = block.get_first_instruction();
            while let Some(current) = instruction {
                instruction = current.get_next_instruction();
                if current.get_opcode() != InstructionOpcode::Call {
                    continue;
                }
                let count = current.get_num_operands();
                if count != 4 {
                    continue;
                }
                let is_mulmod = current
                    .get_operand(count - 1)
                    .and_then(|operand| operand.value())
                    .map(|value| value.into_pointer_value())
                    .and_then(|callee| callee.get_name().to_str().ok().map(str::to_owned))
                    .is_some_and(|name| name == MULMOD);
                if is_mulmod {
                    calls.push(current);
                }
            }
        }
        for call in calls {
            let modulus = call
                .get_operand(2)
                .and_then(|operand| operand.value())
                .map(|value| value.into_int_value())
                .filter(|value| value.is_const())
                .as_ref()
                .and_then(|value| parse_wide_constant(value, 256));
            let Some(modulus) = modulus else { continue };

            let builder = context.create_builder();
            builder.position_before(&call);
            let arg = |index| {
                call.get_operand(index)
                    .expect("mulmod argument")
                    .value()
                    .expect("mulmod argument is a value")
                    .into_int_value()
            };
            let (a, b) = (arg(0), arg(1));
            let t = modulus.bits() as u32;
            // HAC 14.42 Barrett needs x = a*b < 2^(2t). Since a,b < 2^256,
            // x < 2^512, so this holds exactly at t = 256 (the crypto field
            // primes). Smaller moduli keep the generic __mulmod path.
            if t != 256 {
                continue;
            }
            let i576 = context
                .custom_width_int_type(NonZeroU32::new(576).expect("nonzero"))
                .expect("i576");
            let result = if modulus <= BigUint::one() {
                i256.const_zero()
            } else {
                let two_t = BigUint::one() << (2 * t);
                let mu = &two_t / &modulus;
                assert!(
                    &mu * &modulus <= two_t && (&mu + BigUint::one()) * &modulus > two_t,
                    "const-division: mulmod reciprocal self-check failed for {modulus}"
                );
                let dec = |v: &BigUint| v.to_str_radix(10);
                let m512 = i512
                    .const_int_from_string(&dec(&modulus), StringRadix::Decimal)
                    .expect("modulus literal");
                let mu576 = i576
                    .const_int_from_string(&dec(&mu), StringRadix::Decimal)
                    .expect("reciprocal literal");
                // x = a*b (< 2^512)
                let a_wide = builder.build_int_z_extend(a, i512, "mm_aw").expect("zext");
                let b_wide = builder.build_int_z_extend(b, i512, "mm_bw").expect("zext");
                let x = builder.build_int_mul(a_wide, b_wide, "mm_x").expect("mul");
                // q = ((x >> (t-1)) * mu) >> (t+1)
                let pre = builder
                    .build_right_shift(x, i512.const_int((t - 1) as u64, false), false, "mm_pre")
                    .expect("shr");
                let pre576 = builder.build_int_z_extend(pre, i576, "mm_pre576").expect("zext");
                let prod = builder.build_int_mul(pre576, mu576, "mm_qmul").expect("mul");
                let q576 = builder
                    .build_right_shift(prod, i576.const_int((t + 1) as u64, false), false, "mm_qs")
                    .expect("shr");
                let q = builder.build_int_truncate(q576, i512, "mm_q").expect("trunc");
                // r = x - q*m, two conditional corrections (bound is 1)
                let qm = builder.build_int_mul(q, m512, "mm_qm").expect("mul");
                let mut r = builder.build_int_sub(x, qm, "mm_r").expect("sub");
                let one = i512.const_int(1, false);
                let _ = one;
                for step in 0..2 {
                    let ge = builder
                        .build_int_compare(inkwell::IntPredicate::UGE, r, m512, &format!("mm_ge{step}"))
                        .expect("cmp");
                    let sub = builder
                        .build_int_sub(r, m512, &format!("mm_rs{step}"))
                        .expect("sub");
                    r = builder
                        .build_select(ge, sub, r, &format!("mm_rsel{step}"))
                        .expect("select")
                        .into_int_value();
                }
                builder.build_int_truncate(r, i256, "mm_res").expect("trunc")
            };
            let replacement = result
                .as_instruction_value()
                .expect("rewrite produces an instruction");
            call.replace_all_uses_with(&replacement);
            call.erase_from_basic_block();
            rewritten += 1;
        }
    }
    rewritten
}

