; Adapted from: https://github.com/matter-labs/era-compiler-llvm/blob/v1.4.0/llvm/lib/Target/EraVM/eravm-stdlib.ll

target datalayout = "e-m:e-p:32:64-i64:64-i128:128-n32:64-S128"
target triple = "riscv64-unknown-unknown-elf"

define i256 @__addmod(i256 %arg1, i256 %arg2, i256 %modulo) #4 {
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

define i256 @__clz(i256 %v) #0 {
entry:
  %vs128 = lshr i256 %v, 128
  %vs128nz = icmp ne i256 %vs128, 0
  %n128 = select i1 %vs128nz, i256 128, i256 256
  %va128 = select i1 %vs128nz, i256 %vs128, i256 %v
  %vs64 = lshr i256 %va128, 64
  %vs64nz = icmp ne i256 %vs64, 0
  %clza64 = sub i256 %n128, 64
  %n64 = select i1 %vs64nz, i256 %clza64, i256 %n128
  %va64 = select i1 %vs64nz, i256 %vs64, i256 %va128
  %vs32 = lshr i256 %va64, 32
  %vs32nz = icmp ne i256 %vs32, 0
  %clza32 = sub i256 %n64, 32
  %n32 = select i1 %vs32nz, i256 %clza32, i256 %n64
  %va32 = select i1 %vs32nz, i256 %vs32, i256 %va64
  %vs16 = lshr i256 %va32, 16
  %vs16nz = icmp ne i256 %vs16, 0
  %clza16 = sub i256 %n32, 16
  %n16 = select i1 %vs16nz, i256 %clza16, i256 %n32
  %va16 = select i1 %vs16nz, i256 %vs16, i256 %va32
  %vs8 = lshr i256 %va16, 8
  %vs8nz = icmp ne i256 %vs8, 0
  %clza8 = sub i256 %n16, 8
  %n8 = select i1 %vs8nz, i256 %clza8, i256 %n16
  %va8 = select i1 %vs8nz, i256 %vs8, i256 %va16
  %vs4 = lshr i256 %va8, 4
  %vs4nz = icmp ne i256 %vs4, 0
  %clza4 = sub i256 %n8, 4
  %n4 = select i1 %vs4nz, i256 %clza4, i256 %n8
  %va4 = select i1 %vs4nz, i256 %vs4, i256 %va8
  %vs2 = lshr i256 %va4, 2
  %vs2nz = icmp ne i256 %vs2, 0
  %clza2 = sub i256 %n4, 2
  %n2 = select i1 %vs2nz, i256 %clza2, i256 %n4
  %va2 = select i1 %vs2nz, i256 %vs2, i256 %va4
  %vs1 = lshr i256 %va2, 1
  %vs1nz = icmp ne i256 %vs1, 0
  %clza1 = sub i256 %n2, 2
  %clzax = sub i256 %n2, %va2
  %result = select i1 %vs1nz, i256 %clza1, i256 %clzax
  ret i256 %result
}

define i256 @__ulongrem(i256 %0, i256 %1, i256 %2) #0 {
  %.not = icmp ult i256 %1, %2
  br i1 %.not, label %4, label %51

4:
  %5 = tail call i256 @__clz(i256 %2)
  %.not61 = icmp eq i256 %5, 0
  br i1 %.not61, label %13, label %6

6:
  %7 = shl i256 %2, %5
  %8 = shl i256 %1, %5
  %9 = sub nuw nsw i256 256, %5
  %10 = lshr i256 %0, %9
  %11 = or i256 %10, %8
  %12 = shl i256 %0, %5
  br label %13

13:
  %.054 = phi i256 [ %7, %6 ], [ %2, %4 ]
  %.053 = phi i256 [ %11, %6 ], [ %1, %4 ]
  %.052 = phi i256 [ %12, %6 ], [ %0, %4 ]
  %14 = lshr i256 %.054, 128
  %15 = udiv i256 %.053, %14
  %16 = urem i256 %.053, %14
  %17 = and i256 %.054, 340282366920938463463374607431768211455
  %18 = lshr i256 %.052, 128
  br label %19

19:
  %.056 = phi i256 [ %15, %13 ], [ %25, %.critedge ]
  %.055 = phi i256 [ %16, %13 ], [ %26, %.critedge ]
  %.not62 = icmp ult i256 %.056, 340282366920938463463374607431768211455
  br i1 %.not62, label %20, label %.critedge

20:
  %21 = mul nuw i256 %.056, %17
  %22 = shl nuw i256 %.055, 128
  %23 = or i256 %22, %18
  %24 = icmp ugt i256 %21, %23
  br i1 %24, label %.critedge, label %27

.critedge:
  %25 = add i256 %.056, -1
  %26 = add i256 %.055, %14
  %.not65 = icmp ult i256 %26, 340282366920938463463374607431768211455
  br i1 %.not65, label %19, label %27

27:
  %.157 = phi i256 [ %25, %.critedge ], [ %.056, %20 ]
  %28 = shl i256 %.053, 128
  %29 = or i256 %18, %28
  %30 = and i256 %.157, 340282366920938463463374607431768211455
  %31 = mul i256 %30, %.054
  %32 = sub i256 %29, %31
  %33 = udiv i256 %32, %14
  %34 = urem i256 %32, %14
  %35 = and i256 %.052, 340282366920938463463374607431768211455
  br label %36

36:
  %.2 = phi i256 [ %33, %27 ], [ %42, %.critedge1 ]
  %.1 = phi i256 [ %34, %27 ], [ %43, %.critedge1 ]
  %.not63 = icmp ult i256 %.2, 340282366920938463463374607431768211455
  br i1 %.not63, label %37, label %.critedge1

37:
  %38 = mul nuw i256 %.2, %17
  %39 = shl i256 %.1, 128
  %40 = or i256 %39, %35
  %41 = icmp ugt i256 %38, %40
  br i1 %41, label %.critedge1, label %44

.critedge1:
  %42 = add i256 %.2, -1
  %43 = add i256 %.1, %14
  %.not64 = icmp ult i256 %43, 340282366920938463463374607431768211455
  br i1 %.not64, label %36, label %44

44:
  %.3 = phi i256 [ %42, %.critedge1 ], [ %.2, %37 ]
  %45 = shl i256 %32, 128
  %46 = or i256 %45, %35
  %47 = and i256 %.3, 340282366920938463463374607431768211455
  %48 = mul i256 %47, %.054
  %49 = sub i256 %46, %48
  %50 = lshr i256 %49, %5
  br label %51

51:
  %.0 = phi i256 [ %50, %44 ], [ -1, %3 ]
  ret i256 %.0
}

define i256 @__mulmod(i256 %arg1, i256 %arg2, i256 %modulo) #0 {
entry:
  %cccond = icmp eq i256 %modulo, 0
  br i1 %cccond, label %ccret, label %entrycont
ccret:
  ret i256 0
entrycont:
  %arg1m = urem i256 %arg1, %modulo
  %arg2m = urem i256 %arg2, %modulo
  %less_then_2_128 = icmp ult i256 %modulo, 340282366920938463463374607431768211456
  br i1 %less_then_2_128, label %fast, label %slow
fast:
  %prod = mul i256 %arg1m, %arg2m
  %prodm = urem i256 %prod, %modulo
  ret i256 %prodm
slow:
  %arg1e = zext i256 %arg1m to i512
  %arg2e = zext i256 %arg2m to i512
  %prode = mul i512 %arg1e, %arg2e
  %prodl = trunc i512 %prode to i256
  %prodeh = lshr i512 %prode, 256
  %prodh = trunc i512 %prodeh to i256
  %res = call i256 @__ulongrem(i256 %prodl, i256 %prodh, i256 %modulo)
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

define i256 @__exp_pow2(i256 %val_log2, i256 %exp) #0 {
entry:
  %shift = mul nuw nsw i256 %val_log2, %exp
  %is_overflow = icmp ugt i256 %shift, 255
  %shift_res = shl nuw i256 1, %shift
  %res = select i1 %is_overflow, i256 0, i256 %shift_res
  ret i256 %res
}


attributes #0 = { mustprogress nofree norecurse nosync nounwind readnone willreturn }
attributes #1 = { argmemonly readonly nofree null_pointer_is_valid }
attributes #2 = { argmemonly mustprogress nofree norecurse nosync nounwind willreturn null_pointer_is_valid }
attributes #3 = { noinline noreturn }
attributes #4 = { alwaysinline mustprogress nofree norecurse nosync nounwind readnone willreturn }
attributes #5 = { noreturn nounwind }