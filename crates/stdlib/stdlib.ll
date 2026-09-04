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
; __udiv_qrnnd_128: divide the 256-bit value (uh:ul) by the 128-bit divisor v.
; Returns { q, r } with q = (uh:ul) / v and r = (uh:ul) mod v, both 128 bits.
;
; Notation: b = 2^64 (this routine's digit base); (x:y) means x*2^w + y where
; w is the bit width of y; / is floor division.
; Preconditions: v != 0 and uh < v, so that q fits in 128 bits.
;
; Knuth TAOCP 4.3.1 Algorithm D / Hacker's Delight 9-4 divlu in base b: a
; 4-digit dividend over a 2-digit divisor, unrolled into two digit steps.
;
;  1. Normalize: s = ctlz(v); vn = v << s (top bit set). Split vn = (vn1:vn0)
;     into 64-bit digits.
;  2. un = (uh:ul) << s, exact in 256-bit arithmetic (uh < v keeps the top s
;     bits free). Take u2 = un >> 128 (the top two digits as one 128-bit
;     value) and the 64-bit digits u1 = (un >> 64) mod b, u0 = un mod b.
;
; First digit step, q1 = (u2:u1) / vn (three digits over two):
;  3. Estimate qhat = min(u2 / vn1, b-1) and rhat = u2 - qhat*vn1 (computed
;     with the capped qhat; exact in 128 bits, may exceed b).
;  4. Correct, exactly two branchless steps (Knuth Thm. 4.3.1B: a normalized
;     divisor never needs more than two):
;       if qhat*vn0 > (rhat:u1) { qhat -= 1; rhat += vn1 }
;       if qhat*vn0 > (rhat:u1) { qhat -= 1 }      (rhat is dead afterwards)
;     The qhat*vn0 products are exact in 128 bits; tests and rhat updates run
;     in 256-bit arithmetic, so an oversized rhat correctly fails the test.
;     q1 = qhat.
;  5. Partial remainder: u21 = (u2:u1) - q1*vn, computed mod 2^128; exact
;     because the true value is a remainder mod vn, hence < 2^128.
;
; Second digit step, q0 = (u21:u0) / vn, identical shape:
;  6. Estimate qhat = min(u21 / vn1, b-1) and rhat = u21 - qhat*vn1.
;  7. The same two branchless corrections, against (rhat:u0). q0 = qhat.
;
;  8. Remainder: r = ((u21:u0) - q0*vn) >> s, computed in 256-bit arithmetic;
;     less than v after the shift, so it truncates to 128 bits losslessly.
;  9. Return { (q1:q0), r }.
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

; One quotient-digit step of Knuth TAOCP 4.3.1 Algorithm D in base B = 2^128.
; Returns q = (uhi:ulo:unext) / vn, the next 128-bit quotient digit of a
; running division by the normalized 256-bit divisor vn.
;
; Notation: B = 2^128; (x:y) = x*B + y, (x:y:z) = x*B^2 + y*B + z; / is
; floor division.
; Preconditions: vn is normalized (bit 255 set) and (uhi:ulo) < vn, which
; guarantees q <= B-1.
; Result: the exact digit q; quotient only. Callers re-derive the partial
; remainder as (uhi:ulo:unext) - q*vn themselves.
;
;  1. Split vn = (vn1:vn0) into 128-bit digits; vn1 has its top bit set.
;  2. Estimate qhat = min((uhi:ulo) / vn1, B-1): __udiv_qrnnd_128 computes
;     (uhi:ulo) / vn1; when uhi >= vn1 (only uhi == vn1 is possible under the
;     precondition) that quotient would be >= B, so qhat is forced to B-1.
;     Normalization bounds the estimate: qhat - 2 <= q <= qhat.
;  3. rhat = (uhi:ulo) - qhat*vn1, exact in 256-bit arithmetic (below 2^129,
;     below 2^130 even after the correction's rhat += vn1; may exceed B when
;     qhat was capped).
;  4. Correct, exactly two branchless steps (Knuth Thm. 4.3.1B: at most two
;     are ever needed for a normalized divisor):
;       if qhat*vn0 > (rhat:unext) { qhat -= 1; rhat += vn1 }
;       if qhat*vn0 > (rhat:unext) { qhat -= 1 }   (rhat is dead afterwards)
;     Each test compares qhat*vn0 (exact 256-bit product, zero-extended)
;     against rhat*B + unext, exact in 384-bit arithmetic; an rhat >= B
;     correctly fails the test.
;  5. Return qhat.
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

; Unsigned 256-bit division. Returns { q, r } with q = u / v and r = u mod v.
;
; Notation: B = 2^128; (x:y) = x*B + y; / is floor division.
; Precondition: v != 0.
;
;  1. Early exit: if u < v, return { 0, u }. Besides the trivial win, this
;     makes __mulmod's argument pre-reductions nearly free when the
;     arguments are already reduced (< modulus), the common case in
;     modular-arithmetic-heavy code.
;  2. Split u = (u1:u0) and v = (v1:v0) into 128-bit digits.
;
; "twoone" path, v1 == 0 (v < 2^128): two digits over one, schoolbook:
;  3. High digit: q1 = u1 / v0, with carry k = u1 - q1*v0 = u1 mod v0
;     (so k < v0).
;  4. Low digit: (q0, r) = __udiv_qrnnd_128(k, u0, v0), dividing (k:u0) by
;     v0; k < v0 satisfies its precondition.
;  5. Return { (q1:q0), r }.
;
; "full" path, v1 != 0 (v >= 2^128): the quotient fits in a single digit
; because u / v < 2^256 / 2^128 = B, so one Algorithm D digit step suffices:
;  6. Normalize: s = ctlz(v1) (0..127); vn = v << s, exact in 256 bits.
;  7. un = u << s, exact in 384-bit arithmetic; split into digits
;     (un2:un1:un0). u < 2^256 <= v*B implies (un2:un1) < vn,
;     __digit_quot's precondition.
;  8. q = __digit_quot(un2, un1, vn, un0) = un / vn = u / v.
;  9. r = (un - q*vn) >> s, exact in 384-bit arithmetic; the result is < v,
;     so it truncates to 256 bits losslessly.
; 10. Return { q, r }.
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

; Remainder of the 512-bit value (phi:plo) modulo m.
; Remainder only: quotient digits are computed to reduce, never assembled.
;
; Notation: B = 2^128; (x:y) is concatenation at the operands' named widths;
; / is floor division.
; Preconditions: phi < m and m >= 2^128. There is deliberately NO
; small-modulus path: the sole caller is __mulmod's 512-bit branch, taken
; only for m >= 2^128 (smaller moduli use __mulmod's 256-bit fast path), and
; it always passes phi < m because phi <= (m-1)^2 / 2^256 < m.
; Result: (phi*2^256 + plo) mod m.
;
; Knuth TAOCP 4.3.1 Algorithm D in base B: a 4-digit dividend over a 2-digit
; divisor, unrolled into two digit steps:
;  1. Normalize: m1 = m >> 128 (nonzero since m >= 2^128); s = ctlz(m1);
;     mn = m << s, exact in 256 bits.
;  2. pn = (phi:plo) << s, exact in 512-bit arithmetic (phi < m keeps the top
;     s bits free). Split into four digits (pn3:pn2:pn1:pn0).
;  3. First digit step: q1 = __digit_quot(pn3, pn2, mn, pn1)
;     = (pn3:pn2:pn1) / mn; its precondition (pn3:pn2) < mn follows from
;     phi < m. Reduce: r1 = (pn3:pn2:pn1) - q1*mn, exact in 384-bit
;     arithmetic; r1 < mn, so it truncates to 256 bits losslessly.
;  4. Second digit step: split r1 = (r1hi:r1lo) into 128-bit digits;
;     q0 = __digit_quot(r1hi, r1lo, mn, pn0) = (r1:pn0) / mn (its
;     precondition r1 < mn holds by construction). Reduce:
;     r2 = (r1:pn0) - q0*mn, exact in 384-bit arithmetic; r2 < mn, truncated
;     to 256 bits.
;  5. Return r2 >> s = (phi:plo) mod m.
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

; Unsigned 256-bit division, quotient only: returns u / v (floor).
; Precondition: v != 0, inherited from the raw i256 udiv this replaces.
; Thin wrapper: the quotient half of __udivrem256. revive emits raw i256
; udiv/urem; the lower_wide_division pass rewrites the non-narrowable
; ones into calls to these wrappers. Kept external so they survive
; optimization until that late pass; unused copies are dropped by the final
; linker --gc-sections.
define i256 @__udiv256(i256 %u, i256 %v) #0 {
  %qr = call { i256, i256 } @__udivrem256(i256 %u, i256 %v)
  %q = extractvalue { i256, i256 } %qr, 0
  ret i256 %q
}

; Unsigned 256-bit remainder, remainder only: returns u mod v.
; Precondition: v != 0, inherited from the raw i256 urem this replaces.
; Thin wrapper: the remainder half of __udivrem256 (see __udiv256 for why
; these wrappers exist).
define i256 @__urem256(i256 %u, i256 %v) #0 {
  %qr = call { i256, i256 } @__udivrem256(i256 %u, i256 %v)
  %r = extractvalue { i256, i256 } %qr, 1
  ret i256 %r
}

; Signed 256-bit division with EVM SDIV semantics -- the quotient truncates
; toward zero. Sign-magnitude wrapper over the unsigned divider.
;
; Preconditions (established by the guard code emitted around the call):
; b != 0, and not (a == -2^255 and b == -1), the two's complement overflow
; pair. Result: q = a / b truncated toward zero.
;
;  1. Sign masks: sign_a = a >>s 255, sign_b = b >>s 255 (arithmetic shift:
;     0 for a nonnegative operand, all-ones i.e. -1 for a negative one).
;  2. Magnitudes: |x| = (x ^ sign_x) - sign_x, the branchless conditional
;     negate (x^0 - 0 = x; x^-1 - (-1) = ~x + 1 = -x). Well defined for
;     every input: |-2^255| = 2^255 fits unsigned.
;  3. q_mag = |a| / |b| via __udiv256; flooring the magnitude quotient is
;     what makes the signed result truncate toward zero.
;  4. Reapply the sign: sign_q = sign_a ^ sign_b (negative iff exactly one
;     operand was); q = (q_mag ^ sign_q) - sign_q.
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

; Signed 256-bit remainder with EVM SMOD semantics. The remainder takes
; the sign of the dividend, so a == b*(a sdiv b) + (a smod b).
; Sign-magnitude wrapper over the unsigned remainder.
;
; Preconditions (established by the guard code emitted around the call):
; b != 0, and not (a == -2^255 and b == -1) -- the non-UB envelope of the
; raw srem this replaces (the emitted SMOD guard actually swaps a -1 divisor
; for 1, so b == -1 never reaches here).
; Result: r with |r| = |a| mod |b| and sign(r) = sign(a), or r == 0.
;
;  1. Sign masks: sign_a = a >>s 255, sign_b = b >>s 255 (0 or all-ones).
;  2. Magnitudes: |a| = (a ^ sign_a) - sign_a, |b| = (b ^ sign_b) - sign_b.
;  3. r_mag = |a| mod |b| via __urem256.
;  4. Reapply the dividend's sign: r = (r_mag ^ sign_a) - sign_a.
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

; EVM MULMOD -- (a * b) mod m with the product taken at full width
; (not mod 2^256), and MULMOD(a, b, 0) = 0.
;
; Preconditions: none (m == 0 is handled here).
; Result: 0 if m == 0, otherwise (a*b) mod m with the exact 512-bit product.
;
;  1. If m == 0, return 0.
;  2. Pre-reduce both arguments: am = a mod m, bm = b mod m via __urem256
;     (its u < v early exit makes this nearly free when the arguments are
;     already reduced, the common case). The residue is unchanged and the
;     product bound shrinks to (m-1)^2.
;  3. Fast path, m < 2^128: am*bm <= (m-1)^2 < 2^256 is exact in 256-bit
;     arithmetic; return (am*bm) mod m via __urem256.
;  4. Slow path, m >= 2^128: p = am*bm computed exactly in 512-bit
;     arithmetic; split p = (phi:plo) into 256-bit halves and return
;     __urem512by256(plo, phi, m). Its preconditions hold: m >= 2^128 from
;     the branch, and phi <= (m-1)^2 / 2^256 < m.
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

; (a*b) mod m via Barrett reduction (HAC 14.42 with b=2, k=t=256), for
; compile-time-constant 256-bit moduli. The compiler rewrites eligible
; __mulmod call sites to this and supplies the reciprocal -- before the
; optimization pipeline for moduli that are already constant (the common
; case), after it for moduli the pipeline exposes as constant.
;
; Preconditions (guaranteed at every rewritten call site; violations return
; garbage but never trap -- the body is straight-line, no udiv/urem/br):
;   2^255 < m < 2^256, m not a power of two,
;   mu_lo = floor(2^512/m) - 2^256, so mu = 2^256 + mu_lo with
;   2^256 < mu < 2^257 (m <= 2^256-1 => mu >= 2^256+1 since
;   (2^256-1)(2^256+1) < 2^512; m > 2^255 => mu < 2^257).
;
; a and b need NOT be pre-reduced: a*b <= (2^256-1)^2 < 2^512 = b^(2k) is
; exactly HAC's operand bound at t = 256, which is why only 256-bit moduli
; are eligible (smaller moduli stay on __mulmod).
;
; Quotient bound: upper -- q3 <= (x/2^255)(2^512/m)/2^257 = x/m, so q3 <= q
; and r0 >= 0; lower -- q1 > x/2^255 - 1 and mu > 2^512/m - 1 give
; q1*mu/2^257 > x/m - x/2^512 - 2^255/m + 2^-257 > x/m - 2, so q3 >= q - 2.
define i256 @__mulmod_barrett(i256 %a, i256 %b, i256 %m, i256 %mu_lo) noinline #0 {
entry:
  %aw  = zext i256 %a to i512
  %bw  = zext i256 %b to i512
  %x   = mul i512 %aw, %bw               ; x < 2^512: no wrap
  %q1  = lshr i512 %x, 255               ; q1 = floor(x/2^255) < 2^257
  ; q2 = q1*mu reconstructed as (q1 << 256) + q1*mu_lo in i576:
  ;   q1 << 256 < 2^513, q1*mu_lo < 2^513, q2 < 2^514 < 2^576: no wrap.
  %q1w = zext i512 %q1 to i576
  %muw = zext i256 %mu_lo to i576
  %hi  = shl i576 %q1w, 256
  %lo  = mul i576 %q1w, %muw
  %q2  = add i576 %hi, %lo
  %q3w = lshr i576 %q2, 257              ; q3 <= floor(x/m), q3 < 2^257
  %q3  = trunc i576 %q3w to i320         ; lossless
  ; r0 = x - q3*m computed mod 2^320: the true value lies in [0, 3m) with
  ; 3m < 2^258, so 320-bit wrapping arithmetic reproduces it exactly.
  %m3  = zext i256 %m to i320
  %x3  = trunc i512 %x to i320
  %q3m = mul i320 %q3, %m3
  %r0  = sub i320 %x3, %q3m
  ; Exactly two conditional corrections (HAC note 14.44: 0 <= q - q3 <= 2):
  ; r0 in [0,3m) -> r1 in [0,2m) -> r2 in [0,m). The bound is tight for this
  ; parameterization (the two deficit terms x/2^512 and 2^255/m each approach
  ; 1), so two is required and sufficient -- do not harmonize with the
  ; division helpers' single correction; these are different theorems.
  %c1  = icmp uge i320 %r0, %m3
  %s1  = sub i320 %r0, %m3
  %r1  = select i1 %c1, i320 %s1, i320 %r0
  %c2  = icmp uge i320 %r1, %m3
  %s2  = sub i320 %r1, %m3
  %r2  = select i1 %c2, i320 %s2, i320 %r1
  %res = trunc i320 %r2 to i256          ; r2 < m < 2^256: lossless
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
