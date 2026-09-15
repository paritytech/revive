; ModuleID = '/home/ubuntu/workspaces/revive/crates/integration/contracts/CallGas.sol:Other'
source_filename = "/home/ubuntu/workspaces/revive/crates/integration/contracts/CallGas.sol:Other"
target datalayout = "e-m:e-p:32:64-p1:32:64-i64:64-i128:128-n32:64-S64"
target triple = "riscv64-unknown-unknown-elf"

module asm
    ".pushsection .polkavm_min_stack_size,\22\22,@progbits"
    "        .word 131072"
    "        .popsection"

%struct.PolkaVM_Metadata = type <{ i8, i32, i32, ptr, i8, i8 }>

$__revive_store_immutable_data_comdat = comdat nodeduplicate

$__revive_panic_comdat = comdat nodeduplicate

@address__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 7, ptr @.str, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@balance__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 7, ptr @.str.1, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@balance_of__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 10, ptr @.str.2, i8 2, i8 0 }>, section ".polkavm_metadata", align 1
@base_fee__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 8, ptr @.str.3, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@block_author__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 12, ptr @.str.4, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@block_hash__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 10, ptr @.str.5, i8 2, i8 0 }>, section ".polkavm_metadata", align 1
@block_number__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 12, ptr @.str.6, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@call_evm__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 8, ptr @.str.7, i8 6, i8 1 }>, section ".polkavm_metadata", align 1
@call_data_copy__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 14, ptr @.str.8, i8 3, i8 0 }>, section ".polkavm_metadata", align 1
@call_data_load__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 14, ptr @.str.9, i8 2, i8 0 }>, section ".polkavm_metadata", align 1
@call_data_size__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 14, ptr @.str.10, i8 0, i8 1 }>, section ".polkavm_metadata", align 1
@caller__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 6, ptr @.str.11, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@chain_id__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 8, ptr @.str.12, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@code_size__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 9, ptr @.str.13, i8 1, i8 1 }>, section ".polkavm_metadata", align 1
@code_hash__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 9, ptr @.str.14, i8 2, i8 0 }>, section ".polkavm_metadata", align 1
@consume_all_gas__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 15, ptr @.str.15, i8 0, i8 0 }>, section ".polkavm_metadata", align 1
@delegate_call_evm__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 17, ptr @.str.16, i8 5, i8 1 }>, section ".polkavm_metadata", align 1
@deposit_event__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 13, ptr @.str.17, i8 4, i8 0 }>, section ".polkavm_metadata", align 1
@gas_limit__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 9, ptr @.str.18, i8 0, i8 1 }>, section ".polkavm_metadata", align 1
@gas_price__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 9, ptr @.str.19, i8 0, i8 1 }>, section ".polkavm_metadata", align 1
@get_immutable_data__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 18, ptr @.str.20, i8 2, i8 0 }>, section ".polkavm_metadata", align 1
@get_storage_or_zero__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 19, ptr @.str.21, i8 3, i8 0 }>, section ".polkavm_metadata", align 1
@hash_keccak_256__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 15, ptr @.str.22, i8 3, i8 0 }>, section ".polkavm_metadata", align 1
@instantiate__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 11, ptr @.str.23, i8 6, i8 1 }>, section ".polkavm_metadata", align 1
@now__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 3, ptr @.str.24, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@origin__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 6, ptr @.str.25, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@seal_return__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 11, ptr @.str.26, i8 3, i8 0 }>, section ".polkavm_metadata", align 1
@ref_time_left__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 13, ptr @.str.27, i8 0, i8 1 }>, section ".polkavm_metadata", align 1
@return_data_copy__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 16, ptr @.str.28, i8 3, i8 0 }>, section ".polkavm_metadata", align 1
@return_data_size__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 16, ptr @.str.29, i8 0, i8 1 }>, section ".polkavm_metadata", align 1
@set_immutable_data__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 18, ptr @.str.30, i8 2, i8 0 }>, section ".polkavm_metadata", align 1
@set_storage_or_clear__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 20, ptr @.str.31, i8 3, i8 1 }>, section ".polkavm_metadata", align 1
@terminate__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 9, ptr @.str.32, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@value_transferred__IMPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 17, ptr @.str.33, i8 1, i8 0 }>, section ".polkavm_metadata", align 1
@.str = private unnamed_addr constant [8 x i8] c"address\00", align 1
@.str.1 = private unnamed_addr constant [8 x i8] c"balance\00", align 1
@.str.2 = private unnamed_addr constant [11 x i8] c"balance_of\00", align 1
@.str.3 = private unnamed_addr constant [9 x i8] c"base_fee\00", align 1
@.str.4 = private unnamed_addr constant [13 x i8] c"block_author\00", align 1
@.str.5 = private unnamed_addr constant [11 x i8] c"block_hash\00", align 1
@.str.6 = private unnamed_addr constant [13 x i8] c"block_number\00", align 1
@.str.7 = private unnamed_addr constant [9 x i8] c"call_evm\00", align 1
@.str.8 = private unnamed_addr constant [15 x i8] c"call_data_copy\00", align 1
@.str.9 = private unnamed_addr constant [15 x i8] c"call_data_load\00", align 1
@.str.10 = private unnamed_addr constant [15 x i8] c"call_data_size\00", align 1
@.str.11 = private unnamed_addr constant [7 x i8] c"caller\00", align 1
@.str.12 = private unnamed_addr constant [9 x i8] c"chain_id\00", align 1
@.str.13 = private unnamed_addr constant [10 x i8] c"code_size\00", align 1
@.str.14 = private unnamed_addr constant [10 x i8] c"code_hash\00", align 1
@.str.15 = private unnamed_addr constant [16 x i8] c"consume_all_gas\00", align 1
@.str.16 = private unnamed_addr constant [18 x i8] c"delegate_call_evm\00", align 1
@.str.17 = private unnamed_addr constant [14 x i8] c"deposit_event\00", align 1
@.str.18 = private unnamed_addr constant [10 x i8] c"gas_limit\00", align 1
@.str.19 = private unnamed_addr constant [10 x i8] c"gas_price\00", align 1
@.str.20 = private unnamed_addr constant [19 x i8] c"get_immutable_data\00", align 1
@.str.21 = private unnamed_addr constant [20 x i8] c"get_storage_or_zero\00", align 1
@.str.22 = private unnamed_addr constant [16 x i8] c"hash_keccak_256\00", align 1
@.str.23 = private unnamed_addr constant [12 x i8] c"instantiate\00", align 1
@.str.24 = private unnamed_addr constant [4 x i8] c"now\00", align 1
@.str.25 = private unnamed_addr constant [7 x i8] c"origin\00", align 1
@.str.26 = private unnamed_addr constant [12 x i8] c"seal_return\00", align 1
@.str.27 = private unnamed_addr constant [14 x i8] c"ref_time_left\00", align 1
@.str.28 = private unnamed_addr constant [17 x i8] c"return_data_copy\00", align 1
@.str.29 = private unnamed_addr constant [17 x i8] c"return_data_size\00", align 1
@.str.30 = private unnamed_addr constant [19 x i8] c"set_immutable_data\00", align 1
@.str.31 = private unnamed_addr constant [21 x i8] c"set_storage_or_clear\00", align 1
@.str.32 = private unnamed_addr constant [10 x i8] c"terminate\00", align 1
@.str.33 = private unnamed_addr constant [18 x i8] c"value_transferred\00", align 1
@calldatasize = internal unnamed_addr global i32 undef
@__heap_memory = internal global [131072 x i8] zeroinitializer
@address_spill_buffer = internal global i160 0
@deploy__EXPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 6, ptr @.str.34, i8 0, i8 0 }>, section ".polkavm_metadata", align 1
@call__EXPORT_METADATA = internal global %struct.PolkaVM_Metadata <{ i8 1, i32 0, i32 4, ptr @.str.1.3, i8 0, i8 0 }>, section ".polkavm_metadata", align 1
@.str.34 = private unnamed_addr constant [7 x i8] c"deploy\00", align 1
@.str.1.3 = private unnamed_addr constant [5 x i8] c"call\00", align 1
@__immutable_data_ptr = global [0 x i256] undef
@__immutable_data_size = local_unnamed_addr global i32 0
@llvm.compiler.used = appending global [36 x ptr] [ptr @address, ptr @balance, ptr @balance_of, ptr @base_fee, ptr @block_author, ptr @block_hash, ptr @block_number, ptr @call_data_copy, ptr @call_data_load, ptr @call_data_size, ptr @call_evm, ptr @caller, ptr @chain_id, ptr @code_hash, ptr @code_size, ptr @consume_all_gas, ptr @delegate_call_evm, ptr @deposit_event, ptr @gas_limit, ptr @gas_price, ptr @get_immutable_data, ptr @get_storage_or_zero, ptr @hash_keccak_256, ptr @instantiate, ptr @now, ptr @origin, ptr @polkavm_export_dummy0, ptr @polkavm_export_dummy1, ptr @ref_time_left, ptr @return_data_copy, ptr @return_data_size, ptr @seal_return, ptr @set_immutable_data, ptr @set_storage_or_clear, ptr @terminate, ptr @value_transferred], section "llvm.metadata"

; Function Attrs: naked noinline nounwind
define internal void @address(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @address__IMPORT_METADATA) #11, !srcloc !11
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @balance(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @balance__IMPORT_METADATA) #11, !srcloc !12
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @balance_of(i32 noundef signext %0, i32 noundef signext %1) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @balance_of__IMPORT_METADATA) #11, !srcloc !13
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @base_fee(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @base_fee__IMPORT_METADATA) #11, !srcloc !14
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @block_author(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @block_author__IMPORT_METADATA) #11, !srcloc !15
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @block_hash(i32 noundef signext %0, i32 noundef signext %1) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @block_hash__IMPORT_METADATA) #11, !srcloc !16
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @block_number(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @block_number__IMPORT_METADATA) #11, !srcloc !17
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @call_data_copy(i32 noundef signext %0, i32 noundef signext %1, i32 noundef signext %2) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @call_data_copy__IMPORT_METADATA) #11, !srcloc !18
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @call_data_load(i32 noundef signext %0, i32 noundef signext %1) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @call_data_load__IMPORT_METADATA) #11, !srcloc !19
  unreachable
}

; Function Attrs: naked noinline nosync nounwind memory(inaccessiblemem: read)
define internal i64 @call_data_size() #1 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @call_data_size__IMPORT_METADATA) #11, !srcloc !20
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal signext i32 @call_evm(i32 noundef signext %0, i32 noundef signext %1, i32 noundef signext %2, i64 noundef %3, i64 noundef %4, i64 noundef %5) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @call_evm__IMPORT_METADATA) #11, !srcloc !21
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @caller(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @caller__IMPORT_METADATA) #11, !srcloc !22
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @chain_id(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @chain_id__IMPORT_METADATA) #11, !srcloc !23
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @code_hash(i32 noundef signext %0, i32 noundef signext %1) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @code_hash__IMPORT_METADATA) #11, !srcloc !24
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal i64 @code_size(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @code_size__IMPORT_METADATA) #11, !srcloc !25
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @consume_all_gas() #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @consume_all_gas__IMPORT_METADATA) #11, !srcloc !26
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal signext i32 @delegate_call_evm(i32 noundef signext %0, i32 noundef signext %1, i64 noundef %2, i64 noundef %3, i64 noundef %4) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @delegate_call_evm__IMPORT_METADATA) #11, !srcloc !27
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @deposit_event(i32 noundef signext %0, i32 noundef signext %1, i32 noundef signext %2, i32 noundef signext %3) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @deposit_event__IMPORT_METADATA) #11, !srcloc !28
  unreachable
}

; Function Attrs: naked noinline nosync nounwind memory(inaccessiblemem: read)
define internal i64 @gas_limit() #1 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @gas_limit__IMPORT_METADATA) #11, !srcloc !29
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal i64 @gas_price() #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @gas_price__IMPORT_METADATA) #11, !srcloc !30
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @get_immutable_data(i32 noundef signext %0, i32 noundef signext %1) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @get_immutable_data__IMPORT_METADATA) #11, !srcloc !31
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @get_storage_or_zero(i32 noundef signext %0, i32 noundef signext %1, i32 noundef signext %2) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @get_storage_or_zero__IMPORT_METADATA) #11, !srcloc !32
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @hash_keccak_256(i32 noundef signext %0, i32 noundef signext %1, i32 noundef signext %2) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @hash_keccak_256__IMPORT_METADATA) #11, !srcloc !33
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal signext i32 @instantiate(i64 noundef %0, i64 noundef %1, i64 noundef %2, i64 noundef %3, i64 noundef %4, i64 noundef %5) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @instantiate__IMPORT_METADATA) #11, !srcloc !34
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @now(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @now__IMPORT_METADATA) #11, !srcloc !35
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @origin(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @origin__IMPORT_METADATA) #11, !srcloc !36
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal i64 @ref_time_left() #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @ref_time_left__IMPORT_METADATA) #11, !srcloc !37
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @return_data_copy(i32 noundef signext %0, i32 noundef signext %1, i32 noundef signext %2) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @return_data_copy__IMPORT_METADATA) #11, !srcloc !38
  unreachable
}

; Function Attrs: naked noinline nosync nounwind memory(inaccessiblemem: read)
define internal i64 @return_data_size() #1 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @return_data_size__IMPORT_METADATA) #11, !srcloc !39
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @seal_return(i32 noundef signext %0, i32 noundef signext %1, i32 noundef signext %2) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @seal_return__IMPORT_METADATA) #11, !srcloc !40
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @set_immutable_data(i32 noundef signext %0, i32 noundef signext %1) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @set_immutable_data__IMPORT_METADATA) #11, !srcloc !41
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal signext i32 @set_storage_or_clear(i32 noundef signext %0, i32 noundef signext %1, i32 noundef signext %2) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @set_storage_or_clear__IMPORT_METADATA) #11, !srcloc !42
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @terminate(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @terminate__IMPORT_METADATA) #11, !srcloc !43
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @value_transferred(i32 noundef signext %0) #0 {
entry:
  tail call void asm sideeffect ".word 0x0000000b\0A.quad $0\0Aret\0A", "i,~{memory}"(ptr nonnull @value_transferred__IMPORT_METADATA) #11, !srcloc !44
  unreachable
}

; Function Attrs: nofree norecurse nosync nounwind memory(argmem: readwrite)
define dso_local noundef ptr @memcpy(ptr nofree noundef returned writeonly captures(ret: address, provenance) %dst, ptr nofree noundef readonly captures(none) %_src, i64 noundef %len) local_unnamed_addr #2 {
entry:
  %tobool.not3 = icmp eq i64 %len, 0
  br i1 %tobool.not3, label %while.end, label %while.body

while.body:                                       ; preds = %entry, %while.body
  %src.06 = phi ptr [ %incdec.ptr, %while.body ], [ %_src, %entry ]
  %dest.05 = phi ptr [ %incdec.ptr1, %while.body ], [ %dst, %entry ]
  %len.addr.04 = phi i64 [ %dec, %while.body ], [ %len, %entry ]
  %dec = add i64 %len.addr.04, -1
  %incdec.ptr = getelementptr inbounds nuw i8, ptr %src.06, i32 1
  %0 = load i8, ptr %src.06, align 1, !tbaa !45, !invariant.load !46
  %incdec.ptr1 = getelementptr inbounds nuw i8, ptr %dest.05, i32 1
  store i8 %0, ptr %dest.05, align 1, !tbaa !45
  %tobool.not = icmp eq i64 %dec, 0
  br i1 %tobool.not, label %while.end, label %while.body, !llvm.loop !47

while.end:                                        ; preds = %while.body, %entry
  ret ptr %dst
}

; Function Attrs: nofree norecurse nosync nounwind memory(argmem: readwrite)
define dso_local noundef ptr @memmove(ptr nofree noundef returned %dst, ptr nofree noundef %src, i64 noundef %n) local_unnamed_addr #2 {
entry:
  %cmp = icmp eq ptr %dst, %src
  br i1 %cmp, label %cleanup, label %if.end

if.end:                                           ; preds = %entry
  %0 = ptrtoint ptr %src to i32
  %1 = zext i32 %0 to i64
  %2 = ptrtoint ptr %dst to i32
  %3 = zext i32 %2 to i64
  %4 = add i64 %n, %3
  %sub1 = sub i64 %1, %4
  %mul = mul i64 %n, -2
  %cmp2.not = icmp ugt i64 %sub1, %mul
  br i1 %cmp2.not, label %if.end4, label %if.then3

if.then3:                                         ; preds = %if.end
  %tobool.not3.i = icmp eq i64 %n, 0
  br i1 %tobool.not3.i, label %cleanup, label %while.body.i

while.body.i:                                     ; preds = %if.then3, %while.body.i
  %src.06.i = phi ptr [ %incdec.ptr.i, %while.body.i ], [ %src, %if.then3 ]
  %dest.05.i = phi ptr [ %incdec.ptr1.i, %while.body.i ], [ %dst, %if.then3 ]
  %len.addr.04.i = phi i64 [ %dec.i, %while.body.i ], [ %n, %if.then3 ]
  %dec.i = add i64 %len.addr.04.i, -1
  %incdec.ptr.i = getelementptr inbounds nuw i8, ptr %src.06.i, i32 1
  %5 = load i8, ptr %src.06.i, align 1, !tbaa !45
  %incdec.ptr1.i = getelementptr inbounds nuw i8, ptr %dest.05.i, i32 1
  store i8 %5, ptr %dest.05.i, align 1, !tbaa !45
  %tobool.not.i = icmp eq i64 %dec.i, 0
  br i1 %tobool.not.i, label %cleanup, label %while.body.i, !llvm.loop !47

if.end4:                                          ; preds = %if.end
  %cmp5 = icmp ult ptr %dst, %src
  %tobool.not38 = icmp eq i64 %n, 0
  br i1 %cmp5, label %for.cond.preheader, label %while.cond.preheader

while.cond.preheader:                             ; preds = %if.end4
  br i1 %tobool.not38, label %cleanup, label %while.body

for.cond.preheader:                               ; preds = %if.end4
  br i1 %tobool.not38, label %cleanup, label %for.body

for.body:                                         ; preds = %for.cond.preheader, %for.body
  %s.041 = phi ptr [ %incdec.ptr, %for.body ], [ %src, %for.cond.preheader ]
  %d.040 = phi ptr [ %incdec.ptr7, %for.body ], [ %dst, %for.cond.preheader ]
  %n.addr.039 = phi i64 [ %dec, %for.body ], [ %n, %for.cond.preheader ]
  %incdec.ptr = getelementptr inbounds nuw i8, ptr %s.041, i32 1
  %6 = load i8, ptr %s.041, align 1, !tbaa !45
  %incdec.ptr7 = getelementptr inbounds nuw i8, ptr %d.040, i32 1
  store i8 %6, ptr %d.040, align 1, !tbaa !45
  %dec = add i64 %n.addr.039, -1
  %tobool.not = icmp eq i64 %dec, 0
  br i1 %tobool.not, label %cleanup, label %for.body, !llvm.loop !49

while.body:                                       ; preds = %while.cond.preheader, %while.body
  %n.addr.137 = phi i64 [ %dec9, %while.body ], [ %n, %while.cond.preheader ]
  %dec9 = add i64 %n.addr.137, -1
  %7 = trunc nuw nsw i64 %dec9 to i32
  %arrayidx = getelementptr inbounds nuw i8, ptr %src, i32 %7
  %8 = load i8, ptr %arrayidx, align 1, !tbaa !45
  %arrayidx10 = getelementptr inbounds nuw i8, ptr %dst, i32 %7
  store i8 %8, ptr %arrayidx10, align 1, !tbaa !45
  %tobool8.not = icmp eq i64 %dec9, 0
  br i1 %tobool8.not, label %cleanup, label %while.body, !llvm.loop !50

cleanup:                                          ; preds = %while.body.i, %while.body, %for.body, %while.cond.preheader, %for.cond.preheader, %if.then3, %entry
  ret ptr %dst
}

; Function Attrs: nofree norecurse nosync nounwind memory(argmem: write)
define dso_local noundef ptr @memset(ptr nofree noundef returned writeonly captures(ret: address, provenance) %b, i32 noundef signext %c, i64 noundef %len) local_unnamed_addr #3 {
entry:
  %conv = trunc i32 %c to i8
  %cmp.not3 = icmp eq i64 %len, 0
  br i1 %cmp.not3, label %while.end, label %while.body

while.body:                                       ; preds = %entry, %while.body
  %len.addr.05 = phi i64 [ %dec, %while.body ], [ %len, %entry ]
  %dest.04 = phi ptr [ %incdec.ptr, %while.body ], [ %b, %entry ]
  %dec = add i64 %len.addr.05, -1
  %incdec.ptr = getelementptr inbounds nuw i8, ptr %dest.04, i32 1
  store i8 %conv, ptr %dest.04, align 1, !tbaa !45
  %cmp.not = icmp eq i64 %dec, 0
  br i1 %cmp.not, label %while.end, label %while.body, !llvm.loop !51

while.end:                                        ; preds = %while.body, %entry
  ret ptr %b
}

; Function Attrs: mustprogress nocallback nocreateundeforpoison nofree nosync nounwind speculatable willreturn memory(none)
declare i256 @llvm.bswap.i256(i256) #4

; Function Attrs: mustprogress nocallback nocreateundeforpoison nofree nosync nounwind speculatable willreturn memory(none)
declare i160 @llvm.bswap.i160(i160) #4

; Function Attrs: mustprogress nofree norecurse nounwind willreturn
define linkonce_odr void @__revive_store_immutable_data() local_unnamed_addr #5 comdat($__revive_store_immutable_data_comdat) {
entry:
  %immutable_data_size_load = load i32, ptr @__immutable_data_size, align 32
  %immutable_data_size_is_zero = icmp eq i32 %immutable_data_size_load, 0
  br i1 %immutable_data_size_is_zero, label %join_return_block, label %write_immutables_block

write_immutables_block:                           ; preds = %entry
  tail call void @set_immutable_data(i32 noundef ptrtoint (ptr @__immutable_data_ptr to i32), i32 noundef %immutable_data_size_load) #11
  br label %join_return_block

join_return_block:                                ; preds = %write_immutables_block, %entry
  ret void
}

; Function Attrs: nofree noreturn nounwind
define void @__entry(i1 %0) local_unnamed_addr #6 {
entry:
  %runtime_api_call_data_size_return_value = tail call i64 @call_data_size() #12
  %call_data_size_truncated = trunc i64 %runtime_api_call_data_size_return_value to i32
  store i32 %call_data_size_truncated, ptr @calldatasize, align 4
  br i1 %0, label %deploy_code_call_block, label %runtime_code_call_block

deploy_code_call_block:                           ; preds = %entry
  tail call fastcc void @__deploy() #13
  unreachable

runtime_code_call_block:                          ; preds = %entry
  tail call fastcc void @__runtime() #13
  unreachable
}

; Function Attrs: nofree noreturn nounwind
define private fastcc void @__deploy() unnamed_addr #6 {
entry:
  store i32 128, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 64), align 16
  tail call void @call_data_copy(i32 noundef ptrtoint (ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 128) to i32), i32 noundef 0, i32 noundef 0) #11
  tail call void @__revive_store_immutable_data() #11
  tail call void @seal_return(i32 noundef 0, i32 noundef ptrtoint (ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 128) to i32), i32 noundef 0) #11
  unreachable
}

; Function Attrs: nofree noreturn nounwind
define private fastcc void @__runtime() unnamed_addr #6 {
entry:
  store i32 128, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 64), align 16
  %calldatasize = load i32, ptr @calldatasize, align 32
  %lt = icmp ugt i32 %calldatasize, 3
  %call_data_output = alloca i256, align 32
  br i1 %lt, label %if_then, label %if_join

if_then:                                          ; preds = %entry
  %ptr_to_xlen = ptrtoint ptr %call_data_output to i32
  call void @call_data_load(i32 noundef %ptr_to_xlen, i32 noundef 0) #11
  %call_data_load_value = load i256, ptr %call_data_output, align 32
  %shr_const = lshr i256 %call_data_load_value, 224
  %narrow_let1 = trunc nuw i256 %shr_const to i32
  switch i32 %narrow_let1, label %if_join [
    i32 1199152552, label %switch_case_0
    i32 -1030204040, label %switch_case_1
  ]

if_join:                                          ; preds = %if_then, %entry
  call void @caller(i32 noundef ptrtoint (ptr @address_spill_buffer to i32)) #11
  %address_value = load i160, ptr @address_spill_buffer, align 32
  %call_byte_swap = call i160 @llvm.bswap.i160(i160 %address_value)
  %address_zext = zext i160 %call_byte_swap to i256
  %sload_word14 = call fastcc i256 @__revive_sload_word(i256 noundef 0) #12
  %and_result15 = and i256 %sload_word14, -1461501637330902918203684832716283019655932542976
  %or_result = or disjoint i256 %and_result15, %address_zext
  call fastcc void @__revive_sstore_word(i256 noundef 0, i256 %or_result) #14
  %sload_word16 = call fastcc i256 @__revive_sload_word(i256 noundef 1) #12
  %gt = icmp eq i256 %sload_word16, -1
  br i1 %gt, label %panic_0x11, label %if_join20

switch_case_0:                                    ; preds = %if_then
  %sload_word = call fastcc i256 @__revive_sload_word(i256 noundef 0) #12
  %and_result = and i256 %sload_word, 1461501637330902918203684832716283019655932542975
  br label %return_shared_80_20

switch_case_1:                                    ; preds = %if_then
  %sload_word12 = call fastcc i256 @__revive_sload_word(i256 noundef 1) #12
  br label %return_shared_80_20

return_shared_80_20:                              ; preds = %switch_case_1, %switch_case_0
  %sload_word12.sink = phi i256 [ %sload_word12, %switch_case_1 ], [ %and_result, %switch_case_0 ]
  call fastcc void @__revive_store_bswap(i256 %sload_word12.sink) #14
  call void @seal_return(i32 noundef 0, i32 noundef ptrtoint (ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 128) to i32), i32 noundef 32) #11
  unreachable

if_join20:                                        ; preds = %if_join
  %addition_result17 = add nuw i256 %sload_word16, 1
  call fastcc void @__revive_sstore_word(i256 noundef 1, i256 %addition_result17) #14
  call void @seal_return(i32 noundef 0, i32 noundef ptrtoint (ptr @__heap_memory to i32), i32 noundef 0) #11
  unreachable

panic_0x11:                                       ; preds = %if_join
  call void @__revive_panic(i256 noundef 17) #13
  unreachable
}

; Function Attrs: nofree noreturn nounwind
define linkonce_odr void @__revive_panic(i256 %0) local_unnamed_addr #6 comdat($__revive_panic_comdat) {
entry:
  store i64 1903904846, ptr @__heap_memory, align 16
  %trunc_0 = trunc i256 %0 to i64
  %swap_x64_value_01 = tail call i64 @llvm.bswap.i64(i64 %trunc_0)
  store i64 %swap_x64_value_01, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 28), align 4
  %shift_1 = lshr i256 %0, 64
  %trunc_1 = trunc i256 %shift_1 to i64
  %swap_x64_value_12 = tail call i64 @llvm.bswap.i64(i64 %trunc_1)
  store i64 %swap_x64_value_12, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 20), align 4
  %shift_2 = lshr i256 %0, 128
  %trunc_2 = trunc i256 %shift_2 to i64
  %swap_x64_value_23 = tail call i64 @llvm.bswap.i64(i64 %trunc_2)
  store i64 %swap_x64_value_23, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 12), align 4
  %shift_3 = lshr i256 %0, 192
  %trunc_3 = trunc nuw i256 %shift_3 to i64
  %swap_x64_value_34 = tail call i64 @llvm.bswap.i64(i64 %trunc_3)
  store i64 %swap_x64_value_34, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 4), align 4
  tail call void @seal_return(i32 noundef 1, i32 noundef ptrtoint (ptr @__heap_memory to i32), i32 noundef 36) #11
  unreachable
}

; Function Attrs: mustprogress nocallback nocreateundeforpoison nofree nosync nounwind speculatable willreturn memory(none)
declare i64 @llvm.bswap.i64(i64) #4

; Function Attrs: minsize nofree noinline nosync nounwind memory(inaccessiblemem: read)
define internal fastcc i256 @__revive_sload_word(i256 noundef range(i256 0, 2) %0) unnamed_addr #7 {
entry:
  %call_byte_swap = shl nuw nsw i256 %0, 248
  %sload_key = alloca i256, align 32
  %sload_value = alloca i256, align 32
  store i256 %call_byte_swap, ptr %sload_key, align 32
  %ptr_to_xlen = ptrtoint ptr %sload_key to i32
  %ptr_to_xlen1 = ptrtoint ptr %sload_value to i32
  call void @get_storage_or_zero(i32 noundef 0, i32 noundef %ptr_to_xlen, i32 noundef %ptr_to_xlen1) #11
  %sload_result = load i256, ptr %sload_value, align 32
  %call_byte_swap2 = call i256 @llvm.bswap.i256(i256 %sload_result)
  ret i256 %call_byte_swap2
}

; Function Attrs: minsize mustprogress nofree noinline norecurse nosync nounwind willreturn memory(write, argmem: none, inaccessiblemem: none, target_mem: none)
define internal fastcc void @__revive_store_bswap(i256 %0) unnamed_addr #8 {
entry:
  %trunc_0 = trunc i256 %0 to i64
  %swap_x64_value_0 = tail call i64 @llvm.bswap.i64(i64 %trunc_0)
  store i64 %swap_x64_value_0, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 152), align 8
  %shift_1 = lshr i256 %0, 64
  %trunc_1 = trunc i256 %shift_1 to i64
  %swap_x64_value_1 = tail call i64 @llvm.bswap.i64(i64 %trunc_1)
  store i64 %swap_x64_value_1, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 144), align 16
  %shift_2 = lshr i256 %0, 128
  %trunc_2 = trunc i256 %shift_2 to i64
  %swap_x64_value_2 = tail call i64 @llvm.bswap.i64(i64 %trunc_2)
  store i64 %swap_x64_value_2, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 136), align 8
  %shift_3 = lshr i256 %0, 192
  %trunc_3 = trunc nuw i256 %shift_3 to i64
  %swap_x64_value_3 = tail call i64 @llvm.bswap.i64(i64 %trunc_3)
  store i64 %swap_x64_value_3, ptr getelementptr inbounds nuw (i8, ptr @__heap_memory, i32 128), align 16
  ret void
}

; Function Attrs: minsize noinline nounwind memory(inaccessiblemem: write)
define internal fastcc void @__revive_sstore_word(i256 noundef range(i256 0, 2) %0, i256 %1) unnamed_addr #9 {
entry:
  %call_byte_swap = shl nuw nsw i256 %0, 248
  %call_byte_swap1 = tail call i256 @llvm.bswap.i256(i256 %1) #15
  %sstore_key = alloca i256, align 32
  %sstore_value = alloca i256, align 32
  store i256 %call_byte_swap, ptr %sstore_key, align 32
  store i256 %call_byte_swap1, ptr %sstore_value, align 32
  %ptr_to_xlen = ptrtoint ptr %sstore_key to i32
  %ptr_to_xlen2 = ptrtoint ptr %sstore_value to i32
  %runtime_api_set_storage_or_clear_return_value = call i32 @set_storage_or_clear(i32 noundef 0, i32 noundef %ptr_to_xlen, i32 noundef %ptr_to_xlen2) #11
  ret void
}

; Function Attrs: naked noinline nounwind
define internal void @polkavm_export_dummy0() #0 {
entry:
  tail call void asm sideeffect ".pushsection .polkavm_exports,\22R\22,@note\0A.byte 1\0A.quad $0\0A.quad $1\0A.popsection\0A", "i,i,~{memory}"(ptr nonnull @deploy__EXPORT_METADATA, ptr nonnull @deploy) #11, !srcloc !52
  unreachable
}

; Function Attrs: naked noinline nounwind
define internal void @polkavm_export_dummy1() #0 {
entry:
  tail call void asm sideeffect ".pushsection .polkavm_exports,\22R\22,@note\0A.byte 1\0A.quad $0\0A.quad $1\0A.popsection\0A", "i,i,~{memory}"(ptr nonnull @call__EXPORT_METADATA, ptr nonnull @call) #11, !srcloc !53
  unreachable
}

; Function Attrs: nofree noreturn nounwind
define internal void @call() #10 {
entry:
  tail call void @__entry(i1 noundef zeroext false) #16
  unreachable
}

; Function Attrs: nofree noreturn nounwind
define internal void @deploy() #10 {
entry:
  tail call void @__entry(i1 noundef zeroext true) #16
  unreachable
}

attributes #0 = { naked noinline nounwind "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "target-cpu"="generic-rv64" "target-features"="+64bit,+a,+c,+e,+fusion-auipc-addi,+fusion-ld-add,+fusion-lui-addi,+m,+relax,+xtheadcondmov,+zaamo,+zalrsc,+zbb,+zca,+zmmul,-b,-d,-experimental-p,-experimental-smpmpmt,-experimental-svukte,-experimental-xqccmt,-experimental-xsfmclic,-experimental-xsfsclic,-experimental-y,-experimental-zibi,-experimental-zicfilp,-experimental-zicfiss,-experimental-zvabd,-experimental-zvbc32e,-experimental-zvdot4a8i,-experimental-zvfbdota32f,-experimental-zvfbfa,-experimental-zvfofp8min,-experimental-zvfqwbdota8f,-experimental-zvfqwdota8f,-experimental-zvfwbdota16bf,-experimental-zvfwdota16bf,-experimental-zvkgs,-experimental-zvqwbdota16i,-experimental-zvqwbdota8i,-experimental-zvqwdota16i,-experimental-zvqwdota8i,-experimental-zvvfmm,-experimental-zvvmm,-experimental-zvvmtls,-experimental-zvvmttls,-experimental-zvzip,-f,-h,-i,-q,-sdext,-sdtrig,-sha,-shcounterenw,-shgatpa,-shlcofideleg,-shtvala,-shvsatpa,-shvstvala,-shvstvecd,-smaia,-smcdeleg,-smcntrpmf,-smcsrind,-smctr,-smdbltrp,-smepmp,-smmpm,-smnpm,-smrnmi,-smstateen,-ssaia,-ssccfg,-ssccptr,-sscofpmf,-sscounterenw,-sscsrind,-ssctr,-ssdbltrp,-ssnpm,-sspm,-ssqosid,-ssstateen,-ssstrict,-sstc,-sstvala,-sstvecd,-ssu64xl,-supm,-svade,-svadu,-svbare,-svinval,-svnapot,-svpbmt,-svrsw60t59b,-svvptc,-v,-xaifet,-xandesbfhcvt,-xandesperf,-xandesvbfhcvt,-xandesvdot,-xandesvpackfph,-xandesvsinth,-xandesvsintload,-xcheriot,-xcvalu,-xcvbi,-xcvbitmanip,-xcvelw,-xcvmac,-xcvmem,-xcvsimd,-xmipscbop,-xmipscmov,-xmipsexectl,-xmipslsp,-xqccmp,-xqci,-xqcia,-xqciac,-xqcibi,-xqcibm,-xqcicli,-xqcicm,-xqcics,-xqcicsr,-xqciint,-xqciio,-xqcilb,-xqcili,-xqcilia,-xqcilo,-xqcilsm,-xqcisim,-xqcisls,-xqcisync,-xsfcease,-xsfmm128t,-xsfmm16t,-xsfmm32a,-xsfmm32a16f,-xsfmm32a32f,-xsfmm32a8f,-xsfmm32a8i,-xsfmm32t,-xsfmm64a64f,-xsfmm64t,-xsfmmbase,-xsfvcp,-xsfvfbfexp16e,-xsfvfexp16e,-xsfvfexp32e,-xsfvfexpa,-xsfvfexpa64e,-xsfvfnrclipxfqf,-xsfvfwmaccqqq,-xsfvqmaccdod,-xsfvqmaccqoq,-xsifivecdiscarddlone,-xsifivecflushdlone,-xsmtvdot,-xsmtvdotii,-xtheadba,-xtheadbb,-xtheadbs,-xtheadcmo,-xtheadfmemidx,-xtheadmac,-xtheadmemidx,-xtheadmempair,-xtheadsync,-xtheadvdot,-xventanacondops,-xwchc,-za128rs,-za64rs,-zabha,-zacas,-zalasr,-zama16b,-zawrs,-zba,-zbc,-zbkb,-zbkc,-zbkx,-zbs,-zcb,-zcd,-zce,-zcf,-zclsd,-zcmop,-zcmp,-zcmt,-zdinx,-zfa,-zfbfmin,-zfh,-zfhmin,-zfinx,-zhinx,-zhinxmin,-zic64b,-zicbom,-zicbop,-zicboz,-ziccamoa,-ziccamoc,-ziccid,-ziccif,-zicclsm,-ziccrse,-zicntr,-zicond,-zicsr,-zifencei,-zihintntl,-zihintpause,-zihpm,-zilsd,-zimop,-zk,-zkn,-zknd,-zkne,-zknh,-zkr,-zks,-zksed,-zksh,-zkt,-ztso,-zvbb,-zvbc,-zve32f,-zve32x,-zve64d,-zve64f,-zve64x,-zvfbfmin,-zvfbfwma,-zvfh,-zvfhmin,-zvkb,-zvkg,-zvkn,-zvknc,-zvkned,-zvkng,-zvknha,-zvknhb,-zvks,-zvksc,-zvksed,-zvksg,-zvksh,-zvkt,-zvl1024b,-zvl128b,-zvl16384b,-zvl2048b,-zvl256b,-zvl32768b,-zvl32b,-zvl4096b,-zvl512b,-zvl64b,-zvl65536b,-zvl8192b" }
attributes #1 = { naked noinline nosync nounwind memory(inaccessiblemem: read) "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "target-cpu"="generic-rv64" "target-features"="+64bit,+a,+c,+e,+fusion-auipc-addi,+fusion-ld-add,+fusion-lui-addi,+m,+relax,+xtheadcondmov,+zaamo,+zalrsc,+zbb,+zca,+zmmul,-b,-d,-experimental-p,-experimental-smpmpmt,-experimental-svukte,-experimental-xqccmt,-experimental-xsfmclic,-experimental-xsfsclic,-experimental-y,-experimental-zibi,-experimental-zicfilp,-experimental-zicfiss,-experimental-zvabd,-experimental-zvbc32e,-experimental-zvdot4a8i,-experimental-zvfbdota32f,-experimental-zvfbfa,-experimental-zvfofp8min,-experimental-zvfqwbdota8f,-experimental-zvfqwdota8f,-experimental-zvfwbdota16bf,-experimental-zvfwdota16bf,-experimental-zvkgs,-experimental-zvqwbdota16i,-experimental-zvqwbdota8i,-experimental-zvqwdota16i,-experimental-zvqwdota8i,-experimental-zvvfmm,-experimental-zvvmm,-experimental-zvvmtls,-experimental-zvvmttls,-experimental-zvzip,-f,-h,-i,-q,-sdext,-sdtrig,-sha,-shcounterenw,-shgatpa,-shlcofideleg,-shtvala,-shvsatpa,-shvstvala,-shvstvecd,-smaia,-smcdeleg,-smcntrpmf,-smcsrind,-smctr,-smdbltrp,-smepmp,-smmpm,-smnpm,-smrnmi,-smstateen,-ssaia,-ssccfg,-ssccptr,-sscofpmf,-sscounterenw,-sscsrind,-ssctr,-ssdbltrp,-ssnpm,-sspm,-ssqosid,-ssstateen,-ssstrict,-sstc,-sstvala,-sstvecd,-ssu64xl,-supm,-svade,-svadu,-svbare,-svinval,-svnapot,-svpbmt,-svrsw60t59b,-svvptc,-v,-xaifet,-xandesbfhcvt,-xandesperf,-xandesvbfhcvt,-xandesvdot,-xandesvpackfph,-xandesvsinth,-xandesvsintload,-xcheriot,-xcvalu,-xcvbi,-xcvbitmanip,-xcvelw,-xcvmac,-xcvmem,-xcvsimd,-xmipscbop,-xmipscmov,-xmipsexectl,-xmipslsp,-xqccmp,-xqci,-xqcia,-xqciac,-xqcibi,-xqcibm,-xqcicli,-xqcicm,-xqcics,-xqcicsr,-xqciint,-xqciio,-xqcilb,-xqcili,-xqcilia,-xqcilo,-xqcilsm,-xqcisim,-xqcisls,-xqcisync,-xsfcease,-xsfmm128t,-xsfmm16t,-xsfmm32a,-xsfmm32a16f,-xsfmm32a32f,-xsfmm32a8f,-xsfmm32a8i,-xsfmm32t,-xsfmm64a64f,-xsfmm64t,-xsfmmbase,-xsfvcp,-xsfvfbfexp16e,-xsfvfexp16e,-xsfvfexp32e,-xsfvfexpa,-xsfvfexpa64e,-xsfvfnrclipxfqf,-xsfvfwmaccqqq,-xsfvqmaccdod,-xsfvqmaccqoq,-xsifivecdiscarddlone,-xsifivecflushdlone,-xsmtvdot,-xsmtvdotii,-xtheadba,-xtheadbb,-xtheadbs,-xtheadcmo,-xtheadfmemidx,-xtheadmac,-xtheadmemidx,-xtheadmempair,-xtheadsync,-xtheadvdot,-xventanacondops,-xwchc,-za128rs,-za64rs,-zabha,-zacas,-zalasr,-zama16b,-zawrs,-zba,-zbc,-zbkb,-zbkc,-zbkx,-zbs,-zcb,-zcd,-zce,-zcf,-zclsd,-zcmop,-zcmp,-zcmt,-zdinx,-zfa,-zfbfmin,-zfh,-zfhmin,-zfinx,-zhinx,-zhinxmin,-zic64b,-zicbom,-zicbop,-zicboz,-ziccamoa,-ziccamoc,-ziccid,-ziccif,-zicclsm,-ziccrse,-zicntr,-zicond,-zicsr,-zifencei,-zihintntl,-zihintpause,-zihpm,-zilsd,-zimop,-zk,-zkn,-zknd,-zkne,-zknh,-zkr,-zks,-zksed,-zksh,-zkt,-ztso,-zvbb,-zvbc,-zve32f,-zve32x,-zve64d,-zve64f,-zve64x,-zvfbfmin,-zvfbfwma,-zvfh,-zvfhmin,-zvkb,-zvkg,-zvkn,-zvknc,-zvkned,-zvkng,-zvknha,-zvknhb,-zvks,-zvksc,-zvksed,-zvksg,-zvksh,-zvkt,-zvl1024b,-zvl128b,-zvl16384b,-zvl2048b,-zvl256b,-zvl32768b,-zvl32b,-zvl4096b,-zvl512b,-zvl64b,-zvl65536b,-zvl8192b" }
attributes #2 = { nofree norecurse nosync nounwind memory(argmem: readwrite) "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "target-cpu"="generic-rv64" "target-features"="+64bit,+a,+c,+e,+fusion-auipc-addi,+fusion-ld-add,+fusion-lui-addi,+m,+relax,+xtheadcondmov,+zaamo,+zalrsc,+zbb,+zca,+zmmul,-b,-d,-experimental-p,-experimental-smpmpmt,-experimental-svukte,-experimental-xqccmt,-experimental-xsfmclic,-experimental-xsfsclic,-experimental-y,-experimental-zibi,-experimental-zicfilp,-experimental-zicfiss,-experimental-zvabd,-experimental-zvbc32e,-experimental-zvdot4a8i,-experimental-zvfbdota32f,-experimental-zvfbfa,-experimental-zvfofp8min,-experimental-zvfqwbdota8f,-experimental-zvfqwdota8f,-experimental-zvfwbdota16bf,-experimental-zvfwdota16bf,-experimental-zvkgs,-experimental-zvqwbdota16i,-experimental-zvqwbdota8i,-experimental-zvqwdota16i,-experimental-zvqwdota8i,-experimental-zvvfmm,-experimental-zvvmm,-experimental-zvvmtls,-experimental-zvvmttls,-experimental-zvzip,-f,-h,-i,-q,-sdext,-sdtrig,-sha,-shcounterenw,-shgatpa,-shlcofideleg,-shtvala,-shvsatpa,-shvstvala,-shvstvecd,-smaia,-smcdeleg,-smcntrpmf,-smcsrind,-smctr,-smdbltrp,-smepmp,-smmpm,-smnpm,-smrnmi,-smstateen,-ssaia,-ssccfg,-ssccptr,-sscofpmf,-sscounterenw,-sscsrind,-ssctr,-ssdbltrp,-ssnpm,-sspm,-ssqosid,-ssstateen,-ssstrict,-sstc,-sstvala,-sstvecd,-ssu64xl,-supm,-svade,-svadu,-svbare,-svinval,-svnapot,-svpbmt,-svrsw60t59b,-svvptc,-v,-xaifet,-xandesbfhcvt,-xandesperf,-xandesvbfhcvt,-xandesvdot,-xandesvpackfph,-xandesvsinth,-xandesvsintload,-xcheriot,-xcvalu,-xcvbi,-xcvbitmanip,-xcvelw,-xcvmac,-xcvmem,-xcvsimd,-xmipscbop,-xmipscmov,-xmipsexectl,-xmipslsp,-xqccmp,-xqci,-xqcia,-xqciac,-xqcibi,-xqcibm,-xqcicli,-xqcicm,-xqcics,-xqcicsr,-xqciint,-xqciio,-xqcilb,-xqcili,-xqcilia,-xqcilo,-xqcilsm,-xqcisim,-xqcisls,-xqcisync,-xsfcease,-xsfmm128t,-xsfmm16t,-xsfmm32a,-xsfmm32a16f,-xsfmm32a32f,-xsfmm32a8f,-xsfmm32a8i,-xsfmm32t,-xsfmm64a64f,-xsfmm64t,-xsfmmbase,-xsfvcp,-xsfvfbfexp16e,-xsfvfexp16e,-xsfvfexp32e,-xsfvfexpa,-xsfvfexpa64e,-xsfvfnrclipxfqf,-xsfvfwmaccqqq,-xsfvqmaccdod,-xsfvqmaccqoq,-xsifivecdiscarddlone,-xsifivecflushdlone,-xsmtvdot,-xsmtvdotii,-xtheadba,-xtheadbb,-xtheadbs,-xtheadcmo,-xtheadfmemidx,-xtheadmac,-xtheadmemidx,-xtheadmempair,-xtheadsync,-xtheadvdot,-xventanacondops,-xwchc,-za128rs,-za64rs,-zabha,-zacas,-zalasr,-zama16b,-zawrs,-zba,-zbc,-zbkb,-zbkc,-zbkx,-zbs,-zcb,-zcd,-zce,-zcf,-zclsd,-zcmop,-zcmp,-zcmt,-zdinx,-zfa,-zfbfmin,-zfh,-zfhmin,-zfinx,-zhinx,-zhinxmin,-zic64b,-zicbom,-zicbop,-zicboz,-ziccamoa,-ziccamoc,-ziccid,-ziccif,-zicclsm,-ziccrse,-zicntr,-zicond,-zicsr,-zifencei,-zihintntl,-zihintpause,-zihpm,-zilsd,-zimop,-zk,-zkn,-zknd,-zkne,-zknh,-zkr,-zks,-zksed,-zksh,-zkt,-ztso,-zvbb,-zvbc,-zve32f,-zve32x,-zve64d,-zve64f,-zve64x,-zvfbfmin,-zvfbfwma,-zvfh,-zvfhmin,-zvkb,-zvkg,-zvkn,-zvknc,-zvkned,-zvkng,-zvknha,-zvknhb,-zvks,-zvksc,-zvksed,-zvksg,-zvksh,-zvkt,-zvl1024b,-zvl128b,-zvl16384b,-zvl2048b,-zvl256b,-zvl32768b,-zvl32b,-zvl4096b,-zvl512b,-zvl64b,-zvl65536b,-zvl8192b" }
attributes #3 = { nofree norecurse nosync nounwind memory(argmem: write) "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "target-cpu"="generic-rv64" "target-features"="+64bit,+a,+c,+e,+fusion-auipc-addi,+fusion-ld-add,+fusion-lui-addi,+m,+relax,+xtheadcondmov,+zaamo,+zalrsc,+zbb,+zca,+zmmul,-b,-d,-experimental-p,-experimental-smpmpmt,-experimental-svukte,-experimental-xqccmt,-experimental-xsfmclic,-experimental-xsfsclic,-experimental-y,-experimental-zibi,-experimental-zicfilp,-experimental-zicfiss,-experimental-zvabd,-experimental-zvbc32e,-experimental-zvdot4a8i,-experimental-zvfbdota32f,-experimental-zvfbfa,-experimental-zvfofp8min,-experimental-zvfqwbdota8f,-experimental-zvfqwdota8f,-experimental-zvfwbdota16bf,-experimental-zvfwdota16bf,-experimental-zvkgs,-experimental-zvqwbdota16i,-experimental-zvqwbdota8i,-experimental-zvqwdota16i,-experimental-zvqwdota8i,-experimental-zvvfmm,-experimental-zvvmm,-experimental-zvvmtls,-experimental-zvvmttls,-experimental-zvzip,-f,-h,-i,-q,-sdext,-sdtrig,-sha,-shcounterenw,-shgatpa,-shlcofideleg,-shtvala,-shvsatpa,-shvstvala,-shvstvecd,-smaia,-smcdeleg,-smcntrpmf,-smcsrind,-smctr,-smdbltrp,-smepmp,-smmpm,-smnpm,-smrnmi,-smstateen,-ssaia,-ssccfg,-ssccptr,-sscofpmf,-sscounterenw,-sscsrind,-ssctr,-ssdbltrp,-ssnpm,-sspm,-ssqosid,-ssstateen,-ssstrict,-sstc,-sstvala,-sstvecd,-ssu64xl,-supm,-svade,-svadu,-svbare,-svinval,-svnapot,-svpbmt,-svrsw60t59b,-svvptc,-v,-xaifet,-xandesbfhcvt,-xandesperf,-xandesvbfhcvt,-xandesvdot,-xandesvpackfph,-xandesvsinth,-xandesvsintload,-xcheriot,-xcvalu,-xcvbi,-xcvbitmanip,-xcvelw,-xcvmac,-xcvmem,-xcvsimd,-xmipscbop,-xmipscmov,-xmipsexectl,-xmipslsp,-xqccmp,-xqci,-xqcia,-xqciac,-xqcibi,-xqcibm,-xqcicli,-xqcicm,-xqcics,-xqcicsr,-xqciint,-xqciio,-xqcilb,-xqcili,-xqcilia,-xqcilo,-xqcilsm,-xqcisim,-xqcisls,-xqcisync,-xsfcease,-xsfmm128t,-xsfmm16t,-xsfmm32a,-xsfmm32a16f,-xsfmm32a32f,-xsfmm32a8f,-xsfmm32a8i,-xsfmm32t,-xsfmm64a64f,-xsfmm64t,-xsfmmbase,-xsfvcp,-xsfvfbfexp16e,-xsfvfexp16e,-xsfvfexp32e,-xsfvfexpa,-xsfvfexpa64e,-xsfvfnrclipxfqf,-xsfvfwmaccqqq,-xsfvqmaccdod,-xsfvqmaccqoq,-xsifivecdiscarddlone,-xsifivecflushdlone,-xsmtvdot,-xsmtvdotii,-xtheadba,-xtheadbb,-xtheadbs,-xtheadcmo,-xtheadfmemidx,-xtheadmac,-xtheadmemidx,-xtheadmempair,-xtheadsync,-xtheadvdot,-xventanacondops,-xwchc,-za128rs,-za64rs,-zabha,-zacas,-zalasr,-zama16b,-zawrs,-zba,-zbc,-zbkb,-zbkc,-zbkx,-zbs,-zcb,-zcd,-zce,-zcf,-zclsd,-zcmop,-zcmp,-zcmt,-zdinx,-zfa,-zfbfmin,-zfh,-zfhmin,-zfinx,-zhinx,-zhinxmin,-zic64b,-zicbom,-zicbop,-zicboz,-ziccamoa,-ziccamoc,-ziccid,-ziccif,-zicclsm,-ziccrse,-zicntr,-zicond,-zicsr,-zifencei,-zihintntl,-zihintpause,-zihpm,-zilsd,-zimop,-zk,-zkn,-zknd,-zkne,-zknh,-zkr,-zks,-zksed,-zksh,-zkt,-ztso,-zvbb,-zvbc,-zve32f,-zve32x,-zve64d,-zve64f,-zve64x,-zvfbfmin,-zvfbfwma,-zvfh,-zvfhmin,-zvkb,-zvkg,-zvkn,-zvknc,-zvkned,-zvkng,-zvknha,-zvknhb,-zvks,-zvksc,-zvksed,-zvksg,-zvksh,-zvkt,-zvl1024b,-zvl128b,-zvl16384b,-zvl2048b,-zvl256b,-zvl32768b,-zvl32b,-zvl4096b,-zvl512b,-zvl64b,-zvl65536b,-zvl8192b" }
attributes #4 = { mustprogress nocallback nocreateundeforpoison nofree nosync nounwind speculatable willreturn memory(none) }
attributes #5 = { mustprogress nofree norecurse nounwind willreturn }
attributes #6 = { nofree noreturn nounwind }
attributes #7 = { minsize nofree noinline nosync nounwind memory(inaccessiblemem: read) }
attributes #8 = { minsize mustprogress nofree noinline norecurse nosync nounwind willreturn memory(write, argmem: none, inaccessiblemem: none, target_mem: none) }
attributes #9 = { minsize noinline nounwind memory(inaccessiblemem: write) }
attributes #10 = { nofree noreturn nounwind "no-builtins" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "target-cpu"="generic-rv64" "target-features"="+64bit,+a,+c,+e,+fusion-auipc-addi,+fusion-ld-add,+fusion-lui-addi,+m,+relax,+xtheadcondmov,+zaamo,+zalrsc,+zbb,+zca,+zmmul,-b,-d,-experimental-p,-experimental-smpmpmt,-experimental-svukte,-experimental-xqccmt,-experimental-xsfmclic,-experimental-xsfsclic,-experimental-y,-experimental-zibi,-experimental-zicfilp,-experimental-zicfiss,-experimental-zvabd,-experimental-zvbc32e,-experimental-zvdot4a8i,-experimental-zvfbdota32f,-experimental-zvfbfa,-experimental-zvfofp8min,-experimental-zvfqwbdota8f,-experimental-zvfqwdota8f,-experimental-zvfwbdota16bf,-experimental-zvfwdota16bf,-experimental-zvkgs,-experimental-zvqwbdota16i,-experimental-zvqwbdota8i,-experimental-zvqwdota16i,-experimental-zvqwdota8i,-experimental-zvvfmm,-experimental-zvvmm,-experimental-zvvmtls,-experimental-zvvmttls,-experimental-zvzip,-f,-h,-i,-q,-sdext,-sdtrig,-sha,-shcounterenw,-shgatpa,-shlcofideleg,-shtvala,-shvsatpa,-shvstvala,-shvstvecd,-smaia,-smcdeleg,-smcntrpmf,-smcsrind,-smctr,-smdbltrp,-smepmp,-smmpm,-smnpm,-smrnmi,-smstateen,-ssaia,-ssccfg,-ssccptr,-sscofpmf,-sscounterenw,-sscsrind,-ssctr,-ssdbltrp,-ssnpm,-sspm,-ssqosid,-ssstateen,-ssstrict,-sstc,-sstvala,-sstvecd,-ssu64xl,-supm,-svade,-svadu,-svbare,-svinval,-svnapot,-svpbmt,-svrsw60t59b,-svvptc,-v,-xaifet,-xandesbfhcvt,-xandesperf,-xandesvbfhcvt,-xandesvdot,-xandesvpackfph,-xandesvsinth,-xandesvsintload,-xcheriot,-xcvalu,-xcvbi,-xcvbitmanip,-xcvelw,-xcvmac,-xcvmem,-xcvsimd,-xmipscbop,-xmipscmov,-xmipsexectl,-xmipslsp,-xqccmp,-xqci,-xqcia,-xqciac,-xqcibi,-xqcibm,-xqcicli,-xqcicm,-xqcics,-xqcicsr,-xqciint,-xqciio,-xqcilb,-xqcili,-xqcilia,-xqcilo,-xqcilsm,-xqcisim,-xqcisls,-xqcisync,-xsfcease,-xsfmm128t,-xsfmm16t,-xsfmm32a,-xsfmm32a16f,-xsfmm32a32f,-xsfmm32a8f,-xsfmm32a8i,-xsfmm32t,-xsfmm64a64f,-xsfmm64t,-xsfmmbase,-xsfvcp,-xsfvfbfexp16e,-xsfvfexp16e,-xsfvfexp32e,-xsfvfexpa,-xsfvfexpa64e,-xsfvfnrclipxfqf,-xsfvfwmaccqqq,-xsfvqmaccdod,-xsfvqmaccqoq,-xsifivecdiscarddlone,-xsifivecflushdlone,-xsmtvdot,-xsmtvdotii,-xtheadba,-xtheadbb,-xtheadbs,-xtheadcmo,-xtheadfmemidx,-xtheadmac,-xtheadmemidx,-xtheadmempair,-xtheadsync,-xtheadvdot,-xventanacondops,-xwchc,-za128rs,-za64rs,-zabha,-zacas,-zalasr,-zama16b,-zawrs,-zba,-zbc,-zbkb,-zbkc,-zbkx,-zbs,-zcb,-zcd,-zce,-zcf,-zclsd,-zcmop,-zcmp,-zcmt,-zdinx,-zfa,-zfbfmin,-zfh,-zfhmin,-zfinx,-zhinx,-zhinxmin,-zic64b,-zicbom,-zicbop,-zicboz,-ziccamoa,-ziccamoc,-ziccid,-ziccif,-zicclsm,-ziccrse,-zicntr,-zicond,-zicsr,-zifencei,-zihintntl,-zihintpause,-zihpm,-zilsd,-zimop,-zk,-zkn,-zknd,-zkne,-zknh,-zkr,-zks,-zksed,-zksh,-zkt,-ztso,-zvbb,-zvbc,-zve32f,-zve32x,-zve64d,-zve64f,-zve64x,-zvfbfmin,-zvfbfwma,-zvfh,-zvfhmin,-zvkb,-zvkg,-zvkn,-zvknc,-zvkned,-zvkng,-zvknha,-zvknhb,-zvks,-zvksc,-zvksed,-zvksg,-zvksh,-zvkt,-zvl1024b,-zvl128b,-zvl16384b,-zvl2048b,-zvl256b,-zvl32768b,-zvl32b,-zvl4096b,-zvl512b,-zvl64b,-zvl65536b,-zvl8192b" }
attributes #11 = { nounwind }
attributes #12 = { nounwind memory(read) }
attributes #13 = { noreturn nounwind }
attributes #14 = { nounwind memory(write) }
attributes #15 = { willreturn }
attributes #16 = { nobuiltin noreturn nounwind "no-builtins" }

!llvm.ident = !{!0}
!llvm.errno.tbaa = !{!1}
!llvm.module.flags = !{!6, !7, !9, !10}

!0 = !{!"Parity Technologies clang version 23.1.0git (https://github.com/llvm/llvm-project.git 88c69e55f3da48bcebdb425f54431f80ce457c8d)"}
!1 = !{!2, !3, i64 0}
!2 = !{!"__libc_errno", !3, i64 0}
!3 = !{!"int", !4, i64 0}
!4 = !{!"omnipotent char", !5, i64 0}
!5 = !{!"Simple C/C++ TBAA"}
!6 = !{i32 1, !"target-abi", !"lp64e"}
!7 = distinct !{i32 6, !"riscv-isa", !8}
!8 = distinct !{!"rv64e2p0_m2p0_a2p1_c2p0_zmmul1p0_zaamo1p0_zalrsc1p0_zca1p0_zbb1p0_xtheadcondmov1p0"}
!9 = !{i32 8, !"SmallDataLimit", i32 0}
!10 = !{i32 4, !"PIE Level", i32 2}
!11 = !{i64 2147551308, i64 2147551340, i64 2147549388}
!12 = !{i64 2147553787, i64 2147553819, i64 2147551867}
!13 = !{i64 2147556651, i64 2147556683, i64 2147554346}
!14 = !{i64 2147559147, i64 2147559179, i64 2147557219}
!15 = !{i64 2147561669, i64 2147561701, i64 2147559709}
!16 = !{i64 2147564548, i64 2147564580, i64 2147562243}
!17 = !{i64 2147567076, i64 2147567108, i64 2147565116}
!18 = !{i64 2147574715, i64 2147574747, i64 2147572017}
!19 = !{i64 2147577632, i64 2147577664, i64 2147575295}
!20 = !{i64 2147579916, i64 2147579948, i64 2147578212}
!21 = !{i64 2147571455, i64 2147571487, i64 2147567650}
!22 = !{i64 2147582408, i64 2147582440, i64 2147580496}
!23 = !{i64 2147584892, i64 2147584924, i64 2147582964}
!24 = !{i64 2147590324, i64 2147590356, i64 2147588027}
!25 = !{i64 2147587462, i64 2147587494, i64 2147585454}
!26 = !{i64 2147592529, i64 2147592561, i64 2147590889}
!27 = !{i64 2147600689, i64 2147600721, i64 2147593112}
!28 = !{i64 2147604329, i64 2147604361, i64 2147601278}
!29 = !{i64 2147606570, i64 2147606602, i64 2147604906}
!30 = !{i64 2147608799, i64 2147608831, i64 2147607135}
!31 = !{i64 2147611733, i64 2147611765, i64 2147609364}
!32 = !{i64 2147615063, i64 2147615095, i64 2147612325}
!33 = !{i64 2147618364, i64 2147618396, i64 2147615658}
!34 = !{i64 2147622776, i64 2147622808, i64 2147618947}
!35 = !{i64 2147625235, i64 2147625267, i64 2147623347}
!36 = !{i64 2147627694, i64 2147627726, i64 2147625782}
!37 = !{i64 2147633191, i64 2147633223, i64 2147631495}
!38 = !{i64 2147636482, i64 2147636514, i64 2147633768}
!39 = !{i64 2147638788, i64 2147638820, i64 2147637068}
!40 = !{i64 2147630924, i64 2147630956, i64 2147628250}
!41 = !{i64 2147641743, i64 2147641775, i64 2147639374}
!42 = !{i64 2147649214, i64 2147649246, i64 2147642335}
!43 = !{i64 2147651748, i64 2147651780, i64 2147649812}
!44 = !{i64 2147654313, i64 2147654345, i64 2147652313}
!45 = !{!4, !4, i64 0}
!46 = !{}
!47 = distinct !{!47, !48}
!48 = !{!"llvm.loop.mustprogress"}
!49 = distinct !{!49, !48}
!50 = distinct !{!50, !48}
!51 = distinct !{!51, !48}
!52 = !{i64 2147510336, i64 2147510393, i64 2147511831, i64 2147511863, i64 2147510446}
!53 = !{i64 2147512338, i64 2147512395, i64 2147513817, i64 2147513849, i64 2147512448}
