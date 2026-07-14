; Adapted from: https://github.com/matter-labs/era-compiler-llvm/blob/v1.4.0/llvm/lib/Target/EraVM/eravm-stdlib.ll

target datalayout = "e-m:e-p:32:64-p1:32:64-i64:64-i128:128-n32:64-S64"
target triple = "riscv64-unknown-none-elf"

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

define private i256 @__clz(i256 %v) #0 {
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

define private i256 @__ulongrem(i256 %lo, i256 %hi, i256 %mod) #10 {
entry:
  %u = alloca [16 x i32], align 32
  %v = alloca [8 x i32], align 32
  %r = alloca [8 x i32], align 32
  store i256 %lo, ptr %u, align 32
  %uhi = getelementptr inbounds i8, ptr %u, i64 32
  store i256 %hi, ptr %uhi, align 32
  store i256 %mod, ptr %v, align 32
  call void @__ulongrem_knuth(ptr %u, ptr %v, ptr %r)
  %res = load i256, ptr %r, align 32
  ret i256 %res
}

define i256 @__mulmod(i256 %arg1, i256 %arg2, i256 %modulo) #20 {
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

define private i256 @__exp_pow2(i256 %val_log2, i256 %exp) #0 {
entry:
  %shift = mul nuw nsw i256 %val_log2, %exp
  %is_overflow = icmp ugt i256 %shift, 255
  %shift_res = shl nuw i256 1, %shift
  %res = select i1 %is_overflow, i256 0, i256 %shift_res
  ret i256 %res
}


attributes #0 = { mustprogress nofree norecurse nosync nounwind readnone willreturn }

define private void @__ulongrem_knuth(ptr noundef readonly captures(none) %u, ptr noundef readonly captures(none) %v, ptr noundef writeonly captures(none) %r) #10 {
entry:
  %un = alloca [17 x i32], align 4
  %vn = alloca [8 x i32], align 4
  call void @llvm.lifetime.start.p0(ptr nonnull %un) #12
  call void @llvm.lifetime.start.p0(ptr nonnull %vn) #12
  %arrayidx = getelementptr i8, ptr %v, i64 28
  %0 = load i32, ptr %arrayidx, align 4
  %cmp1 = icmp eq i32 %0, 0
  br i1 %cmp1, label %while.body, label %while.end

while.body:                                       ; preds = %entry
  %arrayidx.1 = getelementptr i8, ptr %v, i64 24
  %1 = load i32, ptr %arrayidx.1, align 4
  %cmp1.1 = icmp eq i32 %1, 0
  br i1 %cmp1.1, label %while.body.1, label %while.end

while.body.1:                                     ; preds = %while.body
  %arrayidx.2 = getelementptr i8, ptr %v, i64 20
  %2 = load i32, ptr %arrayidx.2, align 4
  %cmp1.2 = icmp eq i32 %2, 0
  br i1 %cmp1.2, label %while.body.2, label %while.end

while.body.2:                                     ; preds = %while.body.1
  %arrayidx.3 = getelementptr i8, ptr %v, i64 16
  %3 = load i32, ptr %arrayidx.3, align 4
  %cmp1.3 = icmp eq i32 %3, 0
  br i1 %cmp1.3, label %while.body.3, label %while.end

while.body.3:                                     ; preds = %while.body.2
  %arrayidx.4 = getelementptr i8, ptr %v, i64 12
  %4 = load i32, ptr %arrayidx.4, align 4
  %cmp1.4 = icmp eq i32 %4, 0
  br i1 %cmp1.4, label %while.body.4, label %while.end

while.body.4:                                     ; preds = %while.body.3
  %arrayidx.5 = getelementptr i8, ptr %v, i64 8
  %5 = load i32, ptr %arrayidx.5, align 4
  %cmp1.5 = icmp eq i32 %5, 0
  br i1 %cmp1.5, label %while.body.5, label %while.end

while.body.5:                                     ; preds = %while.body.4
  %arrayidx.6 = getelementptr i8, ptr %v, i64 4
  %6 = load i32, ptr %arrayidx.6, align 4
  %cmp1.6 = icmp eq i32 %6, 0
  %spec.select554 = select i1 %cmp1.6, i32 1, i32 2
  br label %while.end

while.end:                                        ; preds = %while.body.5, %while.body.4, %while.body.3, %while.body.2, %while.body.1, %while.body, %entry
  %cmp20 = phi i1 [ false, %while.body.4 ], [ false, %entry ], [ false, %while.body ], [ %cmp1.6, %while.body.5 ], [ false, %while.body.1 ], [ false, %while.body.3 ], [ false, %while.body.2 ]
  %n.0.lcssa = phi i32 [ 3, %while.body.4 ], [ 8, %entry ], [ 7, %while.body ], [ %spec.select554, %while.body.5 ], [ 6, %while.body.1 ], [ 4, %while.body.3 ], [ 5, %while.body.2 ]
  %arrayidx7 = getelementptr i8, ptr %u, i64 60
  %7 = load i32, ptr %arrayidx7, align 4
  %cmp8 = icmp eq i32 %7, 0
  br i1 %cmp8, label %while.body10, label %if.end

while.body10:                                     ; preds = %while.end
  %arrayidx7.1 = getelementptr i8, ptr %u, i64 56
  %8 = load i32, ptr %arrayidx7.1, align 4
  %cmp8.1 = icmp eq i32 %8, 0
  br i1 %cmp8.1, label %while.body10.1, label %if.end

while.body10.1:                                   ; preds = %while.body10
  %arrayidx7.2 = getelementptr i8, ptr %u, i64 52
  %9 = load i32, ptr %arrayidx7.2, align 4
  %cmp8.2 = icmp eq i32 %9, 0
  br i1 %cmp8.2, label %while.body10.2, label %if.end

while.body10.2:                                   ; preds = %while.body10.1
  %arrayidx7.3 = getelementptr i8, ptr %u, i64 48
  %10 = load i32, ptr %arrayidx7.3, align 4
  %cmp8.3 = icmp eq i32 %10, 0
  br i1 %cmp8.3, label %while.body10.3, label %if.end

while.body10.3:                                   ; preds = %while.body10.2
  %arrayidx7.4 = getelementptr i8, ptr %u, i64 44
  %11 = load i32, ptr %arrayidx7.4, align 4
  %cmp8.4 = icmp eq i32 %11, 0
  br i1 %cmp8.4, label %while.body10.4, label %if.end

while.body10.4:                                   ; preds = %while.body10.3
  %arrayidx7.5 = getelementptr i8, ptr %u, i64 40
  %12 = load i32, ptr %arrayidx7.5, align 4
  %cmp8.5 = icmp eq i32 %12, 0
  br i1 %cmp8.5, label %while.body10.5, label %if.end

while.body10.5:                                   ; preds = %while.body10.4
  %arrayidx7.6 = getelementptr i8, ptr %u, i64 36
  %13 = load i32, ptr %arrayidx7.6, align 4
  %cmp8.6 = icmp eq i32 %13, 0
  br i1 %cmp8.6, label %while.body10.6, label %if.end

while.body10.6:                                   ; preds = %while.body10.5
  %arrayidx7.7 = getelementptr i8, ptr %u, i64 32
  %14 = load i32, ptr %arrayidx7.7, align 4
  %cmp8.7 = icmp eq i32 %14, 0
  br i1 %cmp8.7, label %while.body10.7, label %if.end

while.body10.7:                                   ; preds = %while.body10.6
  %arrayidx7.8 = getelementptr i8, ptr %u, i64 28
  %15 = load i32, ptr %arrayidx7.8, align 4
  %cmp8.8 = icmp eq i32 %15, 0
  br i1 %cmp8.8, label %while.body10.8, label %if.end

while.body10.8:                                   ; preds = %while.body10.7
  %arrayidx7.9 = getelementptr i8, ptr %u, i64 24
  %16 = load i32, ptr %arrayidx7.9, align 4
  %cmp8.9.not = icmp eq i32 %16, 0
  br i1 %cmp8.9.not, label %while.body10.9, label %while.end12

while.body10.9:                                   ; preds = %while.body10.8
  %arrayidx7.10 = getelementptr i8, ptr %u, i64 20
  %17 = load i32, ptr %arrayidx7.10, align 4
  %cmp8.10 = icmp eq i32 %17, 0
  br i1 %cmp8.10, label %while.body10.10, label %while.end12

while.body10.10:                                  ; preds = %while.body10.9
  %arrayidx7.11 = getelementptr i8, ptr %u, i64 16
  %18 = load i32, ptr %arrayidx7.11, align 4
  %cmp8.11 = icmp eq i32 %18, 0
  br i1 %cmp8.11, label %while.body10.11, label %while.end12

while.body10.11:                                  ; preds = %while.body10.10
  %arrayidx7.12 = getelementptr i8, ptr %u, i64 12
  %19 = load i32, ptr %arrayidx7.12, align 4
  %cmp8.12 = icmp eq i32 %19, 0
  br i1 %cmp8.12, label %while.body10.12, label %while.end12

while.body10.12:                                  ; preds = %while.body10.11
  %arrayidx7.13 = getelementptr i8, ptr %u, i64 8
  %20 = load i32, ptr %arrayidx7.13, align 4
  %cmp8.13 = icmp eq i32 %20, 0
  br i1 %cmp8.13, label %while.body10.13, label %while.end12

while.body10.13:                                  ; preds = %while.body10.12
  %arrayidx7.14 = getelementptr i8, ptr %u, i64 4
  %21 = load i32, ptr %arrayidx7.14, align 4
  %cmp8.14 = icmp eq i32 %21, 0
  br i1 %cmp8.14, label %while.end12.thread479, label %while.end12

while.end12:                                      ; preds = %while.body10.13, %while.body10.12, %while.body10.11, %while.body10.10, %while.body10.9, %while.body10.8
  %cmp15.2 = phi i1 [ true, %while.body10.11 ], [ true, %while.body10.12 ], [ true, %while.body10.10 ], [ false, %while.body10.13 ], [ true, %while.body10.9 ], [ true, %while.body10.8 ]
  %cmp15.3 = phi i1 [ true, %while.body10.11 ], [ false, %while.body10.12 ], [ true, %while.body10.10 ], [ false, %while.body10.13 ], [ true, %while.body10.9 ], [ true, %while.body10.8 ]
  %cmp15.4 = phi i1 [ false, %while.body10.11 ], [ false, %while.body10.12 ], [ true, %while.body10.10 ], [ false, %while.body10.13 ], [ true, %while.body10.9 ], [ true, %while.body10.8 ]
  %cmp15.5 = phi i1 [ false, %while.body10.11 ], [ false, %while.body10.12 ], [ false, %while.body10.10 ], [ false, %while.body10.13 ], [ true, %while.body10.9 ], [ true, %while.body10.8 ]
  %m.0.lcssa = phi i32 [ 4, %while.body10.11 ], [ 3, %while.body10.12 ], [ 5, %while.body10.10 ], [ 2, %while.body10.13 ], [ 6, %while.body10.9 ], [ 7, %while.body10.8 ]
  %cmp13 = icmp samesign ult i32 %m.0.lcssa, %n.0.lcssa
  br i1 %cmp13, label %cond.end.1, label %if.end

while.end12.thread479:                            ; preds = %while.body10.13
  %cmp13487 = icmp samesign ugt i32 %n.0.lcssa, 1
  br i1 %cmp13487, label %cond.end.5.thread, label %if.end

cond.end.5.thread:                                ; preds = %while.end12.thread479
  %22 = load i32, ptr %u, align 4
  store i32 %22, ptr %r, align 4
  %arrayidx19.1511 = getelementptr inbounds nuw i8, ptr %r, i64 4
  store i32 0, ptr %arrayidx19.1511, align 4
  %arrayidx19.2521 = getelementptr inbounds nuw i8, ptr %r, i64 8
  store i32 0, ptr %arrayidx19.2521, align 4
  %arrayidx19.3533 = getelementptr inbounds nuw i8, ptr %r, i64 12
  store i32 0, ptr %arrayidx19.3533, align 4
  %arrayidx19.4542 = getelementptr inbounds nuw i8, ptr %r, i64 16
  store i32 0, ptr %arrayidx19.4542, align 4
  %arrayidx19.5548 = getelementptr inbounds nuw i8, ptr %r, i64 20
  store i32 0, ptr %arrayidx19.5548, align 4
  br label %cleanup.sink.split

cond.end.1:                                       ; preds = %while.end12
  %23 = load i32, ptr %u, align 4
  store i32 %23, ptr %r, align 4
  %arrayidx17.1 = getelementptr inbounds nuw i8, ptr %u, i64 4
  %24 = load i32, ptr %arrayidx17.1, align 4
  %arrayidx19.1 = getelementptr inbounds nuw i8, ptr %r, i64 4
  store i32 %24, ptr %arrayidx19.1, align 4
  br i1 %cmp15.2, label %cond.true.2, label %cond.end.2

cond.true.2:                                      ; preds = %cond.end.1
  %arrayidx17.2 = getelementptr inbounds nuw i8, ptr %u, i64 8
  %25 = load i32, ptr %arrayidx17.2, align 4
  %arrayidx19.2525 = getelementptr inbounds nuw i8, ptr %r, i64 8
  store i32 %25, ptr %arrayidx19.2525, align 4
  br i1 %cmp15.3, label %cond.true.3, label %cond.end.3

cond.end.2:                                       ; preds = %cond.end.1
  %arrayidx19.2 = getelementptr inbounds nuw i8, ptr %r, i64 8
  store i32 0, ptr %arrayidx19.2, align 4
  br i1 %cmp15.3, label %cond.true.3, label %cond.end.3

cond.true.3:                                      ; preds = %cond.true.2, %cond.end.2
  %arrayidx17.3 = getelementptr inbounds nuw i8, ptr %u, i64 12
  %26 = load i32, ptr %arrayidx17.3, align 4
  %arrayidx19.3536 = getelementptr inbounds nuw i8, ptr %r, i64 12
  store i32 %26, ptr %arrayidx19.3536, align 4
  br i1 %cmp15.4, label %cond.true.4, label %cond.end.4

cond.end.3:                                       ; preds = %cond.true.2, %cond.end.2
  %arrayidx19.3 = getelementptr inbounds nuw i8, ptr %r, i64 12
  store i32 0, ptr %arrayidx19.3, align 4
  br i1 %cmp15.4, label %cond.true.4, label %cond.end.4

cond.true.4:                                      ; preds = %cond.true.3, %cond.end.3
  %arrayidx17.4 = getelementptr inbounds nuw i8, ptr %u, i64 16
  %27 = load i32, ptr %arrayidx17.4, align 4
  %arrayidx19.4544 = getelementptr inbounds nuw i8, ptr %r, i64 16
  store i32 %27, ptr %arrayidx19.4544, align 4
  br i1 %cmp15.5, label %cond.true.5, label %cond.end.5

cond.end.4:                                       ; preds = %cond.true.3, %cond.end.3
  %arrayidx19.4 = getelementptr inbounds nuw i8, ptr %r, i64 16
  store i32 0, ptr %arrayidx19.4, align 4
  br i1 %cmp15.5, label %cond.true.5, label %cond.end.5

cond.true.5:                                      ; preds = %cond.true.4, %cond.end.4
  %arrayidx17.5 = getelementptr inbounds nuw i8, ptr %u, i64 20
  %28 = load i32, ptr %arrayidx17.5, align 4
  %arrayidx19.5549 = getelementptr inbounds nuw i8, ptr %r, i64 20
  store i32 %28, ptr %arrayidx19.5549, align 4
  br i1 %cmp8.9.not, label %cleanup.sink.split, label %cond.true.6

cond.end.5:                                       ; preds = %cond.true.4, %cond.end.4
  %arrayidx19.5 = getelementptr inbounds nuw i8, ptr %r, i64 20
  store i32 0, ptr %arrayidx19.5, align 4
  br i1 %cmp8.9.not, label %cleanup.sink.split, label %cond.true.6

cond.true.6:                                      ; preds = %cond.true.5, %cond.end.5
  %arrayidx17.6 = getelementptr inbounds nuw i8, ptr %u, i64 24
  %29 = load i32, ptr %arrayidx17.6, align 4
  br label %cleanup.sink.split

if.end:                                           ; preds = %while.body10.6, %while.body10.5, %while.body10.4, %while.body10.3, %while.body10.2, %while.body10.7, %while.body10.1, %while.body10, %while.end, %while.end12.thread479, %while.end12
  %m.0.lcssa478 = phi i32 [ 1, %while.end12.thread479 ], [ %m.0.lcssa, %while.end12 ], [ 9, %while.body10.6 ], [ 10, %while.body10.5 ], [ 11, %while.body10.4 ], [ 12, %while.body10.3 ], [ 13, %while.body10.2 ], [ 8, %while.body10.7 ], [ 14, %while.body10.1 ], [ 15, %while.body10 ], [ 16, %while.end ]
  %cmp87412477 = phi i1 [ false, %while.end12.thread479 ], [ true, %while.end12 ], [ true, %while.body10.6 ], [ true, %while.body10.5 ], [ true, %while.body10.4 ], [ true, %while.body10.3 ], [ true, %while.body10.2 ], [ true, %while.body10.7 ], [ true, %while.body10.1 ], [ true, %while.body10 ], [ true, %while.end ]
  br i1 %cmp20, label %for.body25.lr.ph, label %if.end45

for.body25.lr.ph:                                 ; preds = %if.end
  %30 = load i32, ptr %v, align 4
  %conv29 = zext i32 %30 to i64
  %31 = zext nneg i32 %m.0.lcssa478 to i64
  br label %for.body25

for.body25:                                       ; preds = %for.body25.lr.ph, %for.body25
  %indvars.iv456 = phi i64 [ %31, %for.body25.lr.ph ], [ %indvars.iv.next457, %for.body25 ]
  %rem.0430 = phi i64 [ 0, %for.body25.lr.ph ], [ %rem30, %for.body25 ]
  %indvars.iv.next457 = add nsw i64 %indvars.iv456, -1
  %shl = shl nuw i64 %rem.0430, 32
  %arrayidx27 = getelementptr inbounds nuw i32, ptr %u, i64 %indvars.iv.next457
  %32 = load i32, ptr %arrayidx27, align 4
  %conv = zext i32 %32 to i64
  %or = or disjoint i64 %shl, %conv
  %rem30 = urem i64 %or, %conv29
  %cmp24 = icmp samesign ugt i64 %indvars.iv456, 1
  br i1 %cmp24, label %for.body25, label %for.end33

for.end33:                                        ; preds = %for.body25
  %conv34 = trunc nuw i64 %rem30 to i32
  store i32 %conv34, ptr %r, align 4
  %arrayidx41 = getelementptr inbounds nuw i8, ptr %r, i64 4
  store i32 0, ptr %arrayidx41, align 4
  %arrayidx41.1 = getelementptr inbounds nuw i8, ptr %r, i64 8
  store i32 0, ptr %arrayidx41.1, align 4
  %arrayidx41.2 = getelementptr inbounds nuw i8, ptr %r, i64 12
  store i32 0, ptr %arrayidx41.2, align 4
  %arrayidx41.3 = getelementptr inbounds nuw i8, ptr %r, i64 16
  store i32 0, ptr %arrayidx41.3, align 4
  %arrayidx41.4 = getelementptr inbounds nuw i8, ptr %r, i64 20
  store i32 0, ptr %arrayidx41.4, align 4
  br label %cleanup.sink.split

if.end45:                                         ; preds = %if.end
  %sub46 = add nsw i32 %n.0.lcssa, -1
  %idxprom47 = zext nneg i32 %sub46 to i64
  %arrayidx48 = getelementptr inbounds nuw i32, ptr %v, i64 %idxprom47
  %33 = load i32, ptr %arrayidx48, align 4
  %cmp.i = icmp eq i32 %33, 0
  br i1 %cmp.i, label %for.body53.lr.ph, label %if.end.i

if.end.i:                                         ; preds = %if.end45
  %cmp1.i = icmp ult i32 %33, 65536
  %shl.i = shl nuw i32 %33, 16
  %spec.select.i = select i1 %cmp1.i, i32 %shl.i, i32 %33
  %spec.select37.i = select i1 %cmp1.i, i32 16, i32 0
  %cmp4.i = icmp ult i32 %spec.select.i, 16777216
  %add6.i = or disjoint i32 %spec.select37.i, 8
  %shl7.i = shl nuw i32 %spec.select.i, 8
  %x.addr.1.i = select i1 %cmp4.i, i32 %shl7.i, i32 %spec.select.i
  %n.1.i = select i1 %cmp4.i, i32 %add6.i, i32 %spec.select37.i
  %cmp9.i = icmp ult i32 %x.addr.1.i, 268435456
  %add11.i = or disjoint i32 %n.1.i, 4
  %shl12.i = shl nuw i32 %x.addr.1.i, 4
  %x.addr.2.i = select i1 %cmp9.i, i32 %shl12.i, i32 %x.addr.1.i
  %n.2.i = select i1 %cmp9.i, i32 %add11.i, i32 %n.1.i
  %cmp14.i = icmp ult i32 %x.addr.2.i, 1073741824
  %add16.i = or disjoint i32 %n.2.i, 2
  %shl17.i = shl nuw i32 %x.addr.2.i, 2
  %x.addr.3.i = select i1 %cmp14.i, i32 %shl17.i, i32 %x.addr.2.i
  %n.3.i = select i1 %cmp14.i, i32 %add16.i, i32 %n.2.i
  %cmp1938.i = icmp sgt i32 %x.addr.3.i, -1
  %add21.i = zext i1 %cmp1938.i to i32
  %n.4.i = add nuw nsw i32 %n.3.i, %add21.i
  br label %for.body53.lr.ph

for.body53.lr.ph:                                 ; preds = %if.end.i, %if.end45
  %retval.0.i = phi i32 [ %n.4.i, %if.end.i ], [ 32, %if.end45 ]
  %sub62 = sub nsw i32 31, %retval.0.i
  %sh_prom = zext i32 %sub62 to i64
  %34 = zext nneg i32 %sub46 to i64
  br label %for.body53

for.body53:                                       ; preds = %for.body53.lr.ph, %for.body53
  %indvars.iv = phi i64 [ %34, %for.body53.lr.ph ], [ %indvars.iv.next, %for.body53 ]
  %arrayidx55 = getelementptr inbounds nuw i32, ptr %v, i64 %indvars.iv
  %35 = load i32, ptr %arrayidx55, align 4
  %shl56 = shl i32 %35, %retval.0.i
  %arrayidx60 = getelementptr i8, ptr %arrayidx55, i64 -4
  %36 = load i32, ptr %arrayidx60, align 4
  %conv61 = zext i32 %36 to i64
  %shr = lshr i64 %conv61, %sh_prom
  %shr63 = lshr i64 %shr, 1
  %37 = trunc nuw nsw i64 %shr63 to i32
  %conv65 = or i32 %shl56, %37
  %arrayidx67 = getelementptr inbounds nuw i32, ptr %vn, i64 %indvars.iv
  store i32 %conv65, ptr %arrayidx67, align 4
  %indvars.iv.next = add nsw i64 %indvars.iv, -1
  %cmp51 = icmp samesign ugt i64 %indvars.iv, 1
  br i1 %cmp51, label %for.body53, label %for.end70

for.end70:                                        ; preds = %for.body53
  %38 = load i32, ptr %v, align 4
  %shl72 = shl i32 %38, %retval.0.i
  store i32 %shl72, ptr %vn, align 4
  %sub74 = add nsw i32 %m.0.lcssa478, -1
  %idxprom75 = zext nneg i32 %sub74 to i64
  %arrayidx76 = getelementptr inbounds nuw i32, ptr %u, i64 %idxprom75
  %39 = load i32, ptr %arrayidx76, align 4
  %conv77 = zext i32 %39 to i64
  %shr80 = lshr i64 %conv77, %sh_prom
  %shr81 = lshr i64 %shr80, 1
  %conv82 = trunc nuw nsw i64 %shr81 to i32
  %idxprom83 = zext nneg i32 %m.0.lcssa478 to i64
  %arrayidx84 = getelementptr inbounds nuw i32, ptr %un, i64 %idxprom83
  store i32 %conv82, ptr %arrayidx84, align 4
  br i1 %cmp87412477, label %for.body89.preheader, label %for.end108

for.body89.preheader:                             ; preds = %for.end70
  %40 = zext nneg i32 %sub74 to i64
  br label %for.body89

for.body89:                                       ; preds = %for.body89.preheader, %for.body89
  %indvars.iv436 = phi i64 [ %40, %for.body89.preheader ], [ %indvars.iv.next437, %for.body89 ]
  %arrayidx91 = getelementptr inbounds nuw i32, ptr %u, i64 %indvars.iv436
  %41 = load i32, ptr %arrayidx91, align 4
  %shl92 = shl i32 %41, %retval.0.i
  %arrayidx96 = getelementptr i8, ptr %arrayidx91, i64 -4
  %42 = load i32, ptr %arrayidx96, align 4
  %conv97 = zext i32 %42 to i64
  %shr100 = lshr i64 %conv97, %sh_prom
  %shr101 = lshr i64 %shr100, 1
  %43 = trunc nuw nsw i64 %shr101 to i32
  %conv103 = or i32 %shl92, %43
  %arrayidx105 = getelementptr inbounds nuw i32, ptr %un, i64 %indvars.iv436
  store i32 %conv103, ptr %arrayidx105, align 4
  %indvars.iv.next437 = add nsw i64 %indvars.iv436, -1
  %cmp87 = icmp samesign ugt i64 %indvars.iv436, 1
  br i1 %cmp87, label %for.body89, label %for.end108

for.end108:                                       ; preds = %for.body89, %for.end70
  %44 = load i32, ptr %u, align 4
  %shl110 = shl i32 %44, %retval.0.i
  store i32 %shl110, ptr %un, align 4
  %sub112 = sub nsw i32 %m.0.lcssa478, %n.0.lcssa
  %cmp114421 = icmp sgt i32 %sub112, -1
  br i1 %cmp114421, label %for.body116.lr.ph, label %for.body235.preheader

for.body116.lr.ph:                                ; preds = %for.end108
  %arrayidx129 = getelementptr inbounds nuw i32, ptr %vn, i64 %idxprom47
  %45 = load i32, ptr %arrayidx129, align 4
  %conv130 = zext i32 %45 to i64
  %46 = zext nneg i32 %n.0.lcssa to i64
  %47 = getelementptr i32, ptr %vn, i64 %46
  %arrayidx141 = getelementptr i8, ptr %47, i64 -8
  %48 = zext nneg i32 %sub112 to i64
  %invariant.gep552 = getelementptr i32, ptr %un, i64 %46
  %wide.trip.count = zext nneg i32 %n.0.lcssa to i64
  %wide.trip.count444 = zext nneg i32 %n.0.lcssa to i64
  br label %for.body116

for.body235.preheader:                            ; preds = %for.inc228, %for.end108
  %wide.trip.count451 = zext nneg i32 %sub46 to i64
  %.pre465 = load i32, ptr %un, align 4
  br label %for.body235

for.body116:                                      ; preds = %for.body116.lr.ph, %for.inc228
  %indvars.iv446 = phi i64 [ %48, %for.body116.lr.ph ], [ %indvars.iv.next447, %for.inc228 ]
  %gep553 = getelementptr i32, ptr %invariant.gep552, i64 %indvars.iv446
  %49 = load i32, ptr %gep553, align 4
  %conv119 = zext i32 %49 to i64
  %shl120 = shl nuw i64 %conv119, 32
  %arrayidx124 = getelementptr i8, ptr %gep553, i64 -4
  %50 = load i32, ptr %arrayidx124, align 4
  %conv125 = zext i32 %50 to i64
  %or126 = or disjoint i64 %shl120, %conv125
  %div = udiv i64 %or126, %conv130
  %mul = mul i64 %div, %conv130
  %sub135 = sub i64 %or126, %mul
  %arrayidx148 = getelementptr i8, ptr %gep553, i64 -8
  br label %while.cond136

while.cond136:                                    ; preds = %while.body153, %for.body116
  %rhat.0 = phi i64 [ %sub135, %for.body116 ], [ %add159, %while.body153 ]
  %qhat.0 = phi i64 [ %div, %for.body116 ], [ %dec154, %while.body153 ]
  %cmp137 = icmp ugt i64 %qhat.0, 4294967295
  br i1 %cmp137, label %while.body153, label %lor.rhs

lor.rhs:                                          ; preds = %while.cond136
  %51 = load i32, ptr %arrayidx141, align 4
  %conv142 = zext i32 %51 to i64
  %mul143 = mul nuw i64 %qhat.0, %conv142
  %shl144 = shl i64 %rhat.0, 32
  %52 = load i32, ptr %arrayidx148, align 4
  %conv149 = zext i32 %52 to i64
  %or150 = or disjoint i64 %shl144, %conv149
  %cmp151 = icmp ugt i64 %mul143, %or150
  br i1 %cmp151, label %while.body153, label %for.body168.preheader

while.body153:                                    ; preds = %while.cond136, %lor.rhs
  %dec154 = add i64 %qhat.0, -1
  %add159 = add i64 %rhat.0, %conv130
  %cmp160 = icmp ugt i64 %add159, 4294967295
  br i1 %cmp160, label %for.body168.preheader, label %while.cond136

for.body168.preheader:                            ; preds = %lor.rhs, %while.body153
  %qhat.1 = phi i64 [ %dec154, %while.body153 ], [ %qhat.0, %lor.rhs ]
  %invariant.gep = getelementptr i32, ptr %un, i64 %indvars.iv446
  br label %for.body168

for.body168:                                      ; preds = %for.body168.preheader, %for.body168
  %indvars.iv438 = phi i64 [ 0, %for.body168.preheader ], [ %indvars.iv.next439, %for.body168 ]
  %k.0415 = phi i64 [ 0, %for.body168.preheader ], [ %sub185, %for.body168 ]
  %arrayidx170 = getelementptr inbounds nuw i32, ptr %vn, i64 %indvars.iv438
  %53 = load i32, ptr %arrayidx170, align 4
  %conv171 = zext i32 %53 to i64
  %mul172 = mul i64 %qhat.1, %conv171
  %gep = getelementptr i32, ptr %invariant.gep, i64 %indvars.iv438
  %54 = load i32, ptr %gep, align 4
  %conv176 = zext i32 %54 to i64
  %and = and i64 %mul172, 4294967295
  %55 = add nuw nsw i64 %k.0415, %and
  %sub178 = sub nsw i64 %conv176, %55
  %conv179 = trunc i64 %sub178 to i32
  store i32 %conv179, ptr %gep, align 4
  %shr183 = lshr i64 %mul172, 32
  %shr184 = ashr i64 %sub178, 32
  %sub185 = sub nsw i64 %shr183, %shr184
  %indvars.iv.next439 = add nuw nsw i64 %indvars.iv438, 1
  %exitcond.not = icmp eq i64 %indvars.iv.next439, %wide.trip.count
  br i1 %exitcond.not, label %for.end188, label %for.body168

for.end188:                                       ; preds = %for.body168
  %.pre = load i32, ptr %gep553, align 4
  %.pre466 = zext i32 %.pre to i64
  %sub193 = sub nsw i64 %.pre466, %sub185
  %conv194 = trunc i64 %sub193 to i32
  store i32 %conv194, ptr %gep553, align 4
  %cmp198 = icmp slt i64 %sub193, 0
  br i1 %cmp198, label %for.body204.preheader, label %for.inc228

for.body204.preheader:                            ; preds = %for.end188
  %invariant.gep550 = getelementptr i32, ptr %un, i64 %indvars.iv446
  br label %for.body204

for.body204:                                      ; preds = %for.body204.preheader, %for.body204
  %indvars.iv441 = phi i64 [ 0, %for.body204.preheader ], [ %indvars.iv.next442, %for.body204 ]
  %k.1418 = phi i64 [ 0, %for.body204.preheader ], [ %shr218, %for.body204 ]
  %gep551 = getelementptr i32, ptr %invariant.gep550, i64 %indvars.iv441
  %56 = load i32, ptr %gep551, align 4
  %conv208 = zext i32 %56 to i64
  %arrayidx210 = getelementptr inbounds nuw i32, ptr %vn, i64 %indvars.iv441
  %57 = load i32, ptr %arrayidx210, align 4
  %conv211 = zext i32 %57 to i64
  %add212 = add nuw nsw i64 %k.1418, %conv208
  %add213 = add nuw nsw i64 %add212, %conv211
  %conv214 = trunc i64 %add213 to i32
  store i32 %conv214, ptr %gep551, align 4
  %shr218 = lshr i64 %add213, 32
  %indvars.iv.next442 = add nuw nsw i64 %indvars.iv441, 1
  %exitcond445.not = icmp eq i64 %indvars.iv.next442, %wide.trip.count444
  br i1 %exitcond445.not, label %for.end221, label %for.body204

for.end221:                                       ; preds = %for.body204
  %.pre464 = load i32, ptr %gep553, align 4
  %conv222 = trunc nuw nsw i64 %shr218 to i32
  %add226 = add i32 %.pre464, %conv222
  store i32 %add226, ptr %gep553, align 4
  br label %for.inc228

for.inc228:                                       ; preds = %for.end188, %for.end221
  %indvars.iv.next447 = add nsw i64 %indvars.iv446, -1
  %cmp114 = icmp sgt i64 %indvars.iv446, 0
  br i1 %cmp114, label %for.body116, label %for.body235.preheader

for.body235:                                      ; preds = %for.body235.preheader, %for.body235
  %58 = phi i32 [ %.pre465, %for.body235.preheader ], [ %59, %for.body235 ]
  %indvars.iv449 = phi i64 [ 0, %for.body235.preheader ], [ %indvars.iv.next450, %for.body235 ]
  %shr238 = lshr i32 %58, %retval.0.i
  %indvars.iv.next450 = add nuw nsw i64 %indvars.iv449, 1
  %arrayidx242 = getelementptr inbounds nuw i32, ptr %un, i64 %indvars.iv.next450
  %59 = load i32, ptr %arrayidx242, align 4
  %conv243 = zext i32 %59 to i64
  %shl246 = shl i64 %conv243, %sh_prom
  %shl246.tr = trunc i64 %shl246 to i32
  %60 = shl i32 %shl246.tr, 1
  %conv249 = or i32 %60, %shr238
  %arrayidx251 = getelementptr inbounds nuw i32, ptr %r, i64 %indvars.iv449
  store i32 %conv249, ptr %arrayidx251, align 4
  %exitcond452.not = icmp eq i64 %indvars.iv.next450, %wide.trip.count451
  br i1 %exitcond452.not, label %for.end254, label %for.body235

for.end254:                                       ; preds = %for.body235
  %arrayidx257 = getelementptr inbounds nuw i32, ptr %un, i64 %idxprom47
  %61 = load i32, ptr %arrayidx257, align 4
  %shr258 = lshr i32 %61, %retval.0.i
  %arrayidx261 = getelementptr inbounds nuw i32, ptr %r, i64 %idxprom47
  store i32 %shr258, ptr %arrayidx261, align 4
  br i1 %cmp1, label %for.body265.preheader, label %cleanup

for.body265.preheader:                            ; preds = %for.end254
  %62 = zext nneg i32 %n.0.lcssa to i64
  br label %for.body265

for.body265:                                      ; preds = %for.body265.preheader, %for.body265
  %indvars.iv453 = phi i64 [ %62, %for.body265.preheader ], [ %indvars.iv.next454, %for.body265 ]
  %arrayidx267 = getelementptr inbounds nuw i32, ptr %r, i64 %indvars.iv453
  store i32 0, ptr %arrayidx267, align 4
  %indvars.iv.next454 = add nuw nsw i64 %indvars.iv453, 1
  %exitcond455.not = icmp eq i64 %indvars.iv.next454, 8
  br i1 %exitcond455.not, label %cleanup, label %for.body265

cleanup.sink.split:                               ; preds = %cond.true.5, %cond.end.5.thread, %cond.true.6, %cond.end.5, %for.end33
  %.sink = phi i32 [ 0, %for.end33 ], [ %29, %cond.true.6 ], [ 0, %cond.end.5 ], [ 0, %cond.end.5.thread ], [ 0, %cond.true.5 ]
  %arrayidx41.5 = getelementptr inbounds nuw i8, ptr %r, i64 24
  store i32 %.sink, ptr %arrayidx41.5, align 4
  %arrayidx41.6 = getelementptr inbounds nuw i8, ptr %r, i64 28
  store i32 0, ptr %arrayidx41.6, align 4
  br label %cleanup

cleanup:                                          ; preds = %for.body265, %cleanup.sink.split, %for.end254
  call void @llvm.lifetime.end.p0(ptr nonnull %vn) #12
  call void @llvm.lifetime.end.p0(ptr nonnull %un) #12
  ret void
}

declare void @llvm.lifetime.start.p0(ptr captures(none)) #11
declare void @llvm.lifetime.end.p0(ptr captures(none)) #11

attributes #10 = { nofree norecurse nosync nounwind memory(argmem: readwrite) }
attributes #11 = { nocallback nofree nosync nounwind willreturn memory(argmem: readwrite) }
attributes #12 = { nounwind }
attributes #20 = { noinline mustprogress nofree norecurse nosync nounwind readnone willreturn }
