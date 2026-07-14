; Adapted from: https://github.com/matter-labs/era-compiler-llvm/blob/v1.4.0/llvm/lib/Target/EraVM/eravm-stdlib.ll

target datalayout = "e-m:e-p:32:64-p1:32:64-i64:64-i128:128-n32:64-S64"
target triple = "riscv64-unknown-none-elf"

declare i128 @llvm.ctlz.i128(i128, i1)

define i256 @__addmod(i256 %arg1, i256 %arg2, i256 %modulo) #0 {
entry:
  %is_zero = icmp eq i256 %modulo, 0
  br i1 %is_zero, label %return, label %addmod

addmod:
  %arg1.fr = freeze i256 %arg1
  %arg1m = urem i256 %arg1.fr, %modulo
  %arg2m = urem i256 %arg2, %modulo
  %sum = add i256 %arg1m, %arg2m
  %obit = icmp ult i256 %sum, %arg1m
  %sum.mod = urem i256 %sum, %modulo
  br i1 %obit, label %overflow, label %return

overflow:
  %mod.inv = xor i256 %modulo, -1
  %sum1 = add i256 %sum, %mod.inv
  %sum.ovf = add i256 %sum1, 1
  br label %return

return:
  %value = phi i256 [0, %entry], [%sum.mod, %addmod], [%sum.ovf, %overflow]
  ret i256 %value
}

; Efficient unsigned 256-bit division / remainder and 512-by-256 reduction.
; Core divide primitive is a 128/64 udiv (efficient __udivti3 libcall),
; never a raw i256 udiv/urem (LLVM expands those into a ~500-instruction
; bit-at-a-time loop). Algorithm: Knuth Algorithm D.
; Assumption at every entry: divisor / modulus != 0 (guarded by the caller).
;
; (uh:ul) / v -> { quotient, remainder }. Precondition: uh < v, 0 < v < 2^128.
define private { i128, i128 } @__udiv_qrnnd_128(i128 %uh, i128 %ul, i128 %v) noinline #0 {
entry:
  %v1 = call i128 @llvm.ctlz.i128(i128 %v, i1 false)
  %v2 = shl i128 %v, %v1
  %v3 = lshr i128 %v2, 64
  %v4 = and i128 %v2, 18446744073709551615
  %v5 = zext i128 %uh to i256
  %v6 = zext i128 %ul to i256
  %v7 = shl i256 %v5, 128
  %v8 = or i256 %v7, %v6
  %v9 = zext i128 %v1 to i256
  %v10 = shl i256 %v8, %v9
  %v11 = lshr i256 %v10, 128
  %v12 = trunc i256 %v11 to i128
  %v13 = lshr i256 %v10, 64
  %v14 = and i256 %v13, 18446744073709551615
  %v15 = trunc i256 %v14 to i128
  %v16 = and i256 %v10, 18446744073709551615
  %v17 = trunc i256 %v16 to i128
  %v18 = udiv i128 %v12, %v3
  %v19 = icmp ugt i128 %v18, 18446744073709551615
  %v20 = select i1 %v19, i128 18446744073709551615, i128 %v18
  %v21 = mul i128 %v20, %v3
  %v22 = sub i128 %v12, %v21
  %v23 = zext i128 %v22 to i256
  %v24 = zext i128 %v3 to i256
  %v25 = zext i128 %v15 to i256
  %v26 = mul i128 %v20, %v4
  %v27 = zext i128 %v26 to i256
  %v28 = shl i256 %v23, 64
  %v29 = add i256 %v28, %v25
  %v30 = icmp ugt i256 %v27, %v29
  %v31 = sub i128 %v20, 1
  %v32 = add i256 %v23, %v24
  %v33 = select i1 %v30, i128 %v31, i128 %v20
  %v34 = select i1 %v30, i256 %v32, i256 %v23
  %v35 = mul i128 %v33, %v4
  %v36 = zext i128 %v35 to i256
  %v37 = shl i256 %v34, 64
  %v38 = add i256 %v37, %v25
  %v39 = icmp ugt i256 %v36, %v38
  %v40 = sub i128 %v33, 1
  %v42 = select i1 %v39, i128 %v40, i128 %v33
  %v44 = shl i128 %v12, 64
  %v45 = add i128 %v44, %v15
  %v46 = mul i128 %v42, %v2
  %v47 = sub i128 %v45, %v46
  %v48 = udiv i128 %v47, %v3
  %v49 = icmp ugt i128 %v48, 18446744073709551615
  %v50 = select i1 %v49, i128 18446744073709551615, i128 %v48
  %v51 = mul i128 %v50, %v3
  %v52 = sub i128 %v47, %v51
  %v53 = zext i128 %v52 to i256
  %v54 = zext i128 %v3 to i256
  %v55 = zext i128 %v17 to i256
  %v56 = mul i128 %v50, %v4
  %v57 = zext i128 %v56 to i256
  %v58 = shl i256 %v53, 64
  %v59 = add i256 %v58, %v55
  %v60 = icmp ugt i256 %v57, %v59
  %v61 = sub i128 %v50, 1
  %v62 = add i256 %v53, %v54
  %v63 = select i1 %v60, i128 %v61, i128 %v50
  %v64 = select i1 %v60, i256 %v62, i256 %v53
  %v65 = mul i128 %v63, %v4
  %v66 = zext i128 %v65 to i256
  %v67 = shl i256 %v64, 64
  %v68 = add i256 %v67, %v55
  %v69 = icmp ugt i256 %v66, %v68
  %v70 = sub i128 %v63, 1
  %v72 = select i1 %v69, i128 %v70, i128 %v63
  %v74 = zext i128 %v47 to i256
  %v75 = shl i256 %v74, 64
  %v76 = zext i128 %v17 to i256
  %v77 = add i256 %v75, %v76
  %v78 = zext i128 %v72 to i256
  %v79 = zext i128 %v2 to i256
  %v80 = mul i256 %v78, %v79
  %v81 = sub i256 %v77, %v80
  %v82 = lshr i256 %v81, %v9
  %v83 = trunc i256 %v82 to i128
  %v84 = shl i128 %v42, 64
  %v85 = or i128 %v84, %v72
  %v86 = insertvalue { i128, i128 } undef, i128 %v85, 0
  %v87 = insertvalue { i128, i128 } %v86, i128 %v83, 1
  ret { i128, i128 } %v87
}

; One normalized 128-bit quotient digit: qhat estimate (guarded cap at 2^128-1)
; plus two branchless correction steps. vn is the normalized 256-bit divisor.
define private i128 @__digit_quot(i128 %uhi, i128 %ulo, i256 %vn, i128 %unext) noinline #0 {
entry:
  %v1 = lshr i256 %vn, 128
  %v2 = trunc i256 %v1 to i128
  %v3 = trunc i256 %vn to i128
  %v4 = call { i128, i128 } @__udiv_qrnnd_128(i128 %uhi, i128 %ulo, i128 %v2)
  %v5 = extractvalue { i128, i128 } %v4, 0
  %v6 = icmp uge i128 %uhi, %v2
  %v7 = select i1 %v6, i128 340282366920938463463374607431768211455, i128 %v5
  %v8 = zext i128 %uhi to i256
  %v9 = zext i128 %ulo to i256
  %v10 = shl i256 %v8, 128
  %v11 = or i256 %v10, %v9
  %v12 = zext i128 %v7 to i256
  %v13 = mul i256 %v12, %v1
  %v14 = sub i256 %v11, %v13
  %v15 = zext i128 %v2 to i256
  %v16 = zext i128 %unext to i384
  %v17 = zext i128 %v7 to i256
  %v18 = zext i128 %v3 to i256
  %v19 = mul i256 %v17, %v18
  %v20 = zext i256 %v19 to i384
  %v21 = zext i256 %v14 to i384
  %v22 = shl i384 %v21, 128
  %v23 = add i384 %v22, %v16
  %v24 = icmp ugt i384 %v20, %v23
  %v25 = sub i128 %v7, 1
  %v26 = add i256 %v14, %v15
  %v27 = select i1 %v24, i128 %v25, i128 %v7
  %v28 = select i1 %v24, i256 %v26, i256 %v14
  %v29 = zext i128 %v27 to i256
  %v30 = zext i128 %v3 to i256
  %v31 = mul i256 %v29, %v30
  %v32 = zext i256 %v31 to i384
  %v33 = zext i256 %v28 to i384
  %v34 = shl i384 %v33, 128
  %v35 = add i384 %v34, %v16
  %v36 = icmp ugt i384 %v32, %v35
  %v37 = sub i128 %v27, 1
  %v39 = select i1 %v36, i128 %v37, i128 %v27
  ret i128 %v39
}

; Full unsigned 256/256 -> { quotient, remainder }. Precondition: v != 0.
define { i256, i256 } @__udivrem256(i256 %u, i256 %v) #0 {
entry:
  ; Early exit: u < v yields quotient 0, remainder u. Makes the __mulmod
  ; argument pre-reductions nearly free when the arguments are already
  ; reduced (< modulus), the common case in modular-arithmetic-heavy code.
  %small = icmp ult i256 %u, %v
  br i1 %small, label %early, label %split
early:
  %e1 = insertvalue { i256, i256 } undef, i256 0, 0
  %e2 = insertvalue { i256, i256 } %e1, i256 %u, 1
  ret { i256, i256 } %e2
split:
  %v1 = lshr i256 %v, 128
  %v2 = trunc i256 %v1 to i128
  %v3 = trunc i256 %v to i128
  %v4 = lshr i256 %u, 128
  %v5 = trunc i256 %v4 to i128
  %v6 = trunc i256 %u to i128
  %v7 = icmp ne i128 %v2, 0
  br i1 %v7, label %full, label %twoone
twoone:
  %v8 = udiv i128 %v5, %v3
  %v9 = mul i128 %v8, %v3
  %v10 = sub i128 %v5, %v9
  %v11 = call { i128, i128 } @__udiv_qrnnd_128(i128 %v10, i128 %v6, i128 %v3)
  %v12 = extractvalue { i128, i128 } %v11, 0
  %v13 = extractvalue { i128, i128 } %v11, 1
  %v14 = zext i128 %v8 to i256
  %v15 = shl i256 %v14, 128
  %v16 = zext i128 %v12 to i256
  %v17 = or i256 %v15, %v16
  %v18 = zext i128 %v13 to i256
  %v19 = insertvalue { i256, i256 } undef, i256 %v17, 0
  %v20 = insertvalue { i256, i256 } %v19, i256 %v18, 1
  ret { i256, i256 } %v20
full:
  %v21 = call i128 @llvm.ctlz.i128(i128 %v2, i1 false)
  %v22 = zext i128 %v21 to i256
  %v23 = shl i256 %v, %v22
  %v24 = zext i256 %u to i384
  %v25 = zext i128 %v21 to i384
  %v26 = shl i384 %v24, %v25
  %v27 = lshr i384 %v26, 256
  %v28 = trunc i384 %v27 to i128
  %v29 = lshr i384 %v26, 128
  %v30 = trunc i384 %v29 to i256
  %v31 = trunc i256 %v30 to i128
  %v32 = trunc i384 %v26 to i128
  %v33 = call i128 @__digit_quot(i128 %v28, i128 %v31, i256 %v23, i128 %v32)
  %v34 = zext i128 %v33 to i256
  %v35 = zext i128 %v28 to i384
  %v36 = zext i128 %v31 to i384
  %v37 = zext i128 %v32 to i384
  %v38 = shl i384 %v35, 256
  %v39 = shl i384 %v36, 128
  %v40 = add i384 %v38, %v39
  %v41 = add i384 %v40, %v37
  %v42 = zext i128 %v33 to i384
  %v43 = zext i256 %v23 to i384
  %v44 = mul i384 %v42, %v43
  %v45 = sub i384 %v41, %v44
  %v46 = lshr i384 %v45, %v25
  %v47 = trunc i384 %v46 to i256
  %v48 = insertvalue { i256, i256 } undef, i256 %v34, 0
  %v49 = insertvalue { i256, i256 } %v48, i256 %v47, 1
  ret { i256, i256 } %v49
}

; Remainder of (phi:plo) mod m. Preconditions: phi < m, m >= 2^128. The sole
; caller is __mulmod's 512-bit branch, which is only taken for m >= 2^128.
; Smaller moduli take __mulmod's 256-bit fast path through __urem256.
define i256 @__urem512by256(i256 %plo, i256 %phi, i256 %m) #0 {
entry:
  %v1 = lshr i256 %m, 128
  %v2 = trunc i256 %v1 to i128
  %v20 = call i128 @llvm.ctlz.i128(i128 %v2, i1 false)
  %v21 = zext i128 %v20 to i256
  %v22 = shl i256 %m, %v21
  %v23 = zext i256 %phi to i512
  %v24 = zext i256 %plo to i512
  %v25 = shl i512 %v23, 256
  %v26 = or i512 %v25, %v24
  %v27 = zext i128 %v20 to i512
  %v28 = shl i512 %v26, %v27
  %v29 = lshr i512 %v28, 384
  %v30 = trunc i512 %v29 to i128
  %v31 = lshr i512 %v28, 256
  %v32 = and i512 %v31, 340282366920938463463374607431768211455
  %v33 = trunc i512 %v32 to i128
  %v34 = lshr i512 %v28, 128
  %v35 = and i512 %v34, 340282366920938463463374607431768211455
  %v36 = trunc i512 %v35 to i128
  %v37 = and i512 %v28, 340282366920938463463374607431768211455
  %v38 = trunc i512 %v37 to i128
  %v39 = zext i256 %v22 to i384
  %v40 = call i128 @__digit_quot(i128 %v30, i128 %v33, i256 %v22, i128 %v36)
  %v41 = zext i128 %v30 to i384
  %v42 = zext i128 %v33 to i384
  %v43 = zext i128 %v36 to i384
  %v44 = shl i384 %v41, 256
  %v45 = shl i384 %v42, 128
  %v46 = add i384 %v44, %v45
  %v47 = add i384 %v46, %v43
  %v48 = zext i128 %v40 to i384
  %v49 = mul i384 %v48, %v39
  %v50 = sub i384 %v47, %v49
  %v51 = trunc i384 %v50 to i256
  %v52 = lshr i256 %v51, 128
  %v53 = trunc i256 %v52 to i128
  %v54 = trunc i256 %v51 to i128
  %v55 = call i128 @__digit_quot(i128 %v53, i128 %v54, i256 %v22, i128 %v38)
  %v56 = zext i256 %v51 to i384
  %v57 = shl i384 %v56, 128
  %v58 = zext i128 %v38 to i384
  %v59 = add i384 %v57, %v58
  %v60 = zext i128 %v55 to i384
  %v61 = mul i384 %v60, %v39
  %v62 = sub i384 %v59, %v61
  %v63 = trunc i384 %v62 to i256
  %v64 = lshr i256 %v63, %v21
  ret i256 %v64
}

; Thin drop-in wrappers. revive emits raw udiv/urem on i256. The
; narrow_divrem_instructions pass rewrites the non-narrowable ones into calls
; to these. Kept external so they survive optimization until that late pass.
; Unused copies are dropped by the final linker --gc-sections.
define i256 @__udiv256(i256 %u, i256 %v) #0 {
  %qr = call { i256, i256 } @__udivrem256(i256 %u, i256 %v)
  %q = extractvalue { i256, i256 } %qr, 0
  ret i256 %q
}

define i256 @__urem256(i256 %u, i256 %v) #0 {
  %qr = call { i256, i256 } @__udivrem256(i256 %u, i256 %v)
  %r = extractvalue { i256, i256 } %qr, 1
  ret i256 %r
}

; Signed 256-bit division via sign-magnitude over the unsigned wrapper. The
; caller only reaches this for a safe operand set (the runtime div body guards
; away divisor == 0 and the INT_MIN / -1 overflow pair), so the magnitude
; divide is exact and the sign-adjusted quotient fits in i256. abs(x) is
; (x ^ (x >>s 255)) - (x >>s 255). The sign mask x >>s 255 is 0 or -1.
define i256 @__sdiv256(i256 %a, i256 %b) #0 {
  %sign_a = ashr i256 %a, 255
  %sign_b = ashr i256 %b, 255
  %xa = xor i256 %a, %sign_a
  %abs_a = sub i256 %xa, %sign_a
  %xb = xor i256 %b, %sign_b
  %abs_b = sub i256 %xb, %sign_b
  %abs_q = call i256 @__udiv256(i256 %abs_a, i256 %abs_b)
  %sign_q = xor i256 %sign_a, %sign_b
  %xq = xor i256 %abs_q, %sign_q
  %q = sub i256 %xq, %sign_q
  ret i256 %q
}

; Signed 256-bit remainder via sign-magnitude. The remainder takes the sign of
; the dividend, matching EVM SMOD.
define i256 @__srem256(i256 %a, i256 %b) #0 {
  %sign_a = ashr i256 %a, 255
  %sign_b = ashr i256 %b, 255
  %xa = xor i256 %a, %sign_a
  %abs_a = sub i256 %xa, %sign_a
  %xb = xor i256 %b, %sign_b
  %abs_b = sub i256 %xb, %sign_b
  %abs_r = call i256 @__urem256(i256 %abs_a, i256 %abs_b)
  %xr = xor i256 %abs_r, %sign_a
  %r = sub i256 %xr, %sign_a
  ret i256 %r
}

define i256 @__mulmod(i256 %arg1, i256 %arg2, i256 %modulo) #0 {
entry:
  %cccond = icmp eq i256 %modulo, 0
  br i1 %cccond, label %ccret, label %entrycont
ccret:
  ret i256 0
entrycont:
  %arg1m = call i256 @__urem256(i256 %arg1, i256 %modulo)
  %arg2m = call i256 @__urem256(i256 %arg2, i256 %modulo)
  %less = icmp ult i256 %modulo, 340282366920938463463374607431768211456
  br i1 %less, label %fast, label %slow
fast:
  %prod = mul i256 %arg1m, %arg2m
  %prodm = call i256 @__urem256(i256 %prod, i256 %modulo)
  ret i256 %prodm
slow:
  %a1e = zext i256 %arg1m to i512
  %a2e = zext i256 %arg2m to i512
  %prode = mul i512 %a1e, %a2e
  %prodl = trunc i512 %prode to i256
  %prodeh = lshr i512 %prode, 256
  %prodh = trunc i512 %prodeh to i256
  %res = call i256 @__urem512by256(i256 %prodl, i256 %prodh, i256 %modulo)
  ret i256 %res
}

define i256 @__signextend(i256 %numbyte, i256 %value) #0 {
entry:
  %is_overflow = icmp uge i256 %numbyte, 31
  br i1 %is_overflow, label %return, label %signextend

signextend:
  %numbit_byte = mul nuw nsw i256 %numbyte, 8
  %numbit = add nsw nuw i256 %numbit_byte, 7
  %numbit_inv = sub i256 256, %numbit
  %signmask = shl i256 1, %numbit
  %valmask = lshr i256 -1, %numbit_inv
  %ext1 = shl i256 -1, %numbit
  %signv = and i256 %signmask, %value
  %sign = icmp ne i256 %signv, 0
  %valclean = and i256 %value, %valmask
  %sext = select i1 %sign, i256 %ext1, i256 0
  %result = or i256 %sext, %valclean
  br label %return

return:
  %signext_res = phi i256 [%value, %entry], [%result, %signextend]
  ret i256 %signext_res
}

define i256 @__exp(i256 %value, i256 %exp) "noinline-oz" #0 {
entry:
  %exp_is_non_zero = icmp eq i256 %exp, 0
  br i1 %exp_is_non_zero, label %return, label %exponent_loop_body

return:
  %exp_res = phi i256 [ 1, %entry ], [ %exp_res.1, %exponent_loop_body ]
  ret i256 %exp_res

exponent_loop_body:
  %exp_res.2 = phi i256 [ %exp_res.1, %exponent_loop_body ], [ 1, %entry ]
  %exp_val = phi i256 [ %exp_val_halved, %exponent_loop_body ], [ %exp, %entry ]
  %val_squared.1 = phi i256 [ %val_squared, %exponent_loop_body ], [ %value, %entry ]
  %odd_test = and i256 %exp_val, 1
  %is_exp_odd = icmp eq i256 %odd_test, 0
  %exp_res.1.interm = select i1 %is_exp_odd, i256 1, i256 %val_squared.1
  %exp_res.1 = mul i256 %exp_res.1.interm, %exp_res.2
  %val_squared = mul i256 %val_squared.1, %val_squared.1
  %exp_val_halved = lshr i256 %exp_val, 1
  %exp_val_is_less_2 = icmp ult i256 %exp_val, 2
  br i1 %exp_val_is_less_2, label %return, label %exponent_loop_body
}

define private i256 @__exp_pow2(i256 %val_log2, i256 %exp) #0 {
entry:
  %shift = mul nuw nsw i256 %val_log2, %exp
  %is_overflow = icmp ugt i256 %shift, 255
  %shift_res = shl nuw i256 1, %shift
  %res = select i1 %is_overflow, i256 0, i256 %shift_res
  ret i256 %res
}


attributes #0 = { mustprogress nofree norecurse nosync nounwind readnone willreturn }
