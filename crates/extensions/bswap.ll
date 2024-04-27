; target datalayout = "e-m:e-p:32:32-i64:64-n32-S128"
; target triple = "riscv32-unknown-unknown-elf"
target datalayout = "e-m:e-p:64:64-i64:64-i128:128-n32:64-S128"
target triple = "riscv64-unknown-unknown-elf"

define dso_local noundef i256 @__bswap(i256 noundef %0) local_unnamed_addr #0 {
  %2 = tail call i256 @llvm.bswap.i256(i256 %0)
  ret i256 %2
}

; Function Attrs: mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i256 @llvm.bswap.i256(i256) #1

attributes #0 = { alwaysinline mustprogress nofree norecurse nosync nounwind willreturn memory(none) }
attributes #1 = { mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none) }
