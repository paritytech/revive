//! IR data structures for the newyork intermediate representation.
//!
//! This module defines the core IR types based on an SSA form with structured
//! control flow, similar to MLIR's SCF dialect. The design preserves high-level
//! structure from Yul while enabling domain-specific optimizations.

use num::BigUint;
use std::collections::BTreeMap;

/// Bit width for integer types.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum BitWidth {
    I1 = 1,
    I8 = 8,
    I32 = 32,
    I64 = 64,
    I160 = 160,
    I256 = 256,
}

impl BitWidth {
    /// Returns the bit width as a u32 for LLVM type construction.
    pub fn bits(self) -> u32 {
        self as u32
    }

    /// Determines the minimum bit width that can hold the given value.
    pub fn from_max_value(value: &BigUint) -> Self {
        if *value <= BigUint::from(1u8) {
            BitWidth::I1
        } else if *value <= BigUint::from(u8::MAX) {
            BitWidth::I8
        } else if *value <= BigUint::from(u32::MAX) {
            BitWidth::I32
        } else if *value <= BigUint::from(u64::MAX) {
            BitWidth::I64
        } else if *value < BigUint::from(2u8).pow(160) {
            BitWidth::I160
        } else {
            BitWidth::I256
        }
    }
}

/// Address space for pointers - distinguishes memory regions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AddressSpace {
    /// EVM heap memory (linear, big-endian).
    Heap,
    /// Native stack allocations (little-endian, optimizable).
    Stack,
    /// Contract storage (key-value, 256-bit slots).
    Storage,
    /// Code/data segment (read-only).
    Code,
}

/// Type of a value in the IR.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    /// Integer with specific bit width.
    Int(BitWidth),
    /// Pointer with address space.
    Ptr(AddressSpace),
    /// No value (for statements/void returns).
    Void,
}

impl Default for Type {
    fn default() -> Self {
        Type::Int(BitWidth::I256)
    }
}

/// Memory region annotation for heap operations.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum MemoryRegion {
    /// Scratch space: addresses 0x00-0x3f (64 bytes).
    Scratch,
    /// Free memory pointer location: address 0x40.
    FreePointerSlot,
    /// Dynamic allocation region: 0x80+.
    Dynamic,
    /// Unknown region (conservative).
    #[default]
    Unknown,
}

impl MemoryRegion {
    /// Determines the memory region from a statically known address.
    pub fn from_address(addr: &BigUint) -> Self {
        let addr_u64 = addr.to_u64_digits();
        if addr_u64.is_empty() || (addr_u64.len() == 1 && addr_u64[0] < 0x40) {
            MemoryRegion::Scratch
        } else if addr_u64.len() == 1 && addr_u64[0] >= 0x40 && addr_u64[0] < 0x60 {
            MemoryRegion::FreePointerSlot
        } else if addr_u64.len() == 1 && addr_u64[0] >= 0x80 {
            MemoryRegion::Dynamic
        } else {
            MemoryRegion::Unknown
        }
    }
}

/// An SSA value reference (index into value table).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ValueId(pub u32);

impl ValueId {
    /// Creates a new value ID.
    pub fn new(id: u32) -> Self {
        ValueId(id)
    }
}

/// A typed SSA value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Value {
    pub id: ValueId,
    pub ty: Type,
}

impl Value {
    /// Creates a new typed value.
    pub fn new(id: ValueId, ty: Type) -> Self {
        Value { id, ty }
    }

    /// Creates an integer value with default I256 type.
    pub fn int(id: ValueId) -> Self {
        Value::new(id, Type::Int(BitWidth::I256))
    }
}

/// Binary operation kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    SDiv,
    Mod,
    SMod,
    Exp,
    AddMod,
    MulMod,
    // Bitwise
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Sar,
    // Comparison (result is I1)
    Lt,
    Gt,
    Slt,
    Sgt,
    Eq,
    // Byte operations
    Byte,
    SignExtend,
}

/// Unary operation kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnaryOp {
    /// Zero check - result is I1.
    IsZero,
    /// Bitwise NOT.
    Not,
    /// Count leading zeros.
    Clz,
}

/// External call kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallKind {
    Call,
    CallCode,
    DelegateCall,
    StaticCall,
}

/// Contract creation kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CreateKind {
    Create,
    Create2,
}

/// Function identifier.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FunctionId(pub u32);

impl FunctionId {
    /// Creates a new function ID.
    pub fn new(id: u32) -> Self {
        FunctionId(id)
    }
}

/// Pure expressions that produce values without side effects.
#[derive(Clone, Debug)]
pub enum Expr {
    /// Literal constant.
    Literal {
        value: BigUint,
        ty: Type,
    },

    /// Reference to an SSA value.
    Var(ValueId),

    /// Binary operation.
    Binary {
        op: BinOp,
        lhs: Value,
        rhs: Value,
    },

    /// Ternary operation (addmod, mulmod).
    Ternary {
        op: BinOp,
        a: Value,
        b: Value,
        n: Value,
    },

    /// Unary operation.
    Unary {
        op: UnaryOp,
        operand: Value,
    },

    // EVM builtins (pure getters)
    CallDataLoad {
        offset: Value,
    },
    CallValue,
    Caller,
    Origin,
    CallDataSize,
    CodeSize,
    GasPrice,
    ExtCodeSize {
        address: Value,
    },
    ReturnDataSize,
    ExtCodeHash {
        address: Value,
    },
    BlockHash {
        number: Value,
    },
    Coinbase,
    Timestamp,
    Number,
    Difficulty,
    GasLimit,
    ChainId,
    SelfBalance,
    BaseFee,
    BlobHash {
        index: Value,
    },
    BlobBaseFee,
    Gas,
    MSize,
    Address,
    Balance {
        address: Value,
    },

    /// Memory load with region annotation.
    MLoad {
        offset: Value,
        region: MemoryRegion,
    },

    /// Storage load with optional static slot.
    SLoad {
        key: Value,
        /// If key is a compile-time constant, store it here for analysis.
        static_slot: Option<BigUint>,
    },

    /// Transient storage load.
    TLoad {
        key: Value,
    },

    /// Function call.
    Call {
        function: FunctionId,
        args: Vec<Value>,
    },

    // Type conversions (explicit)
    Truncate {
        value: Value,
        to: BitWidth,
    },
    ZeroExtend {
        value: Value,
        to: BitWidth,
    },
    SignExtendTo {
        value: Value,
        to: BitWidth,
    },

    /// Keccak256 hash (pure but expensive).
    Keccak256 {
        offset: Value,
        length: Value,
    },

    /// Keccak256 hash of two 256-bit words stored at scratch memory.
    /// Equivalent to: mstore(0, word0); mstore(32, word1); keccak256(0, 64)
    /// but lowered to a single function call to avoid code duplication.
    Keccak256Pair {
        word0: Value,
        word1: Value,
    },

    /// Keccak256 hash of one 256-bit word stored at scratch memory.
    /// Equivalent to: mstore(0, word0); keccak256(0, 32)
    /// but lowered to a single function call to avoid code duplication.
    Keccak256Single {
        word0: Value,
    },

    /// Data offset (for deployed bytecode).
    DataOffset {
        id: String,
    },

    /// Data size (for deployed bytecode).
    DataSize {
        id: String,
    },

    /// Load immutable variable.
    LoadImmutable {
        /// The immutable variable identifier.
        key: String,
    },

    /// Linker symbol - returns the address of an external library.
    LinkerSymbol {
        /// The library path (e.g., "contracts/Library.sol:L").
        path: String,
    },
}

/// Switch case.
#[derive(Clone, Debug)]
pub struct SwitchCase {
    pub value: BigUint,
    pub body: Region,
}

/// A region is a block that can yield values.
#[derive(Clone, Debug, Default)]
pub struct Region {
    /// Statements in this region.
    pub statements: Vec<Statement>,
    /// Values yielded by this region (for structured control flow).
    pub yields: Vec<Value>,
}

impl Region {
    /// Creates a new empty region.
    pub fn new() -> Self {
        Region::default()
    }

    /// Adds a statement to this region.
    pub fn push(&mut self, stmt: Statement) {
        self.statements.push(stmt);
    }
}

/// A basic block of statements (no yields - for function bodies).
#[derive(Clone, Debug, Default)]
pub struct Block {
    pub statements: Vec<Statement>,
}

impl Block {
    /// Creates a new empty block.
    pub fn new() -> Self {
        Block::default()
    }

    /// Adds a statement to this block.
    pub fn push(&mut self, stmt: Statement) {
        self.statements.push(stmt);
    }
}

/// Statements with effects and structured control flow.
#[derive(Clone, Debug)]
pub enum Statement {
    // SSA binding
    /// SSA binding: let x, y, z = expr
    Let {
        bindings: Vec<ValueId>,
        value: Expr,
    },

    // Memory operations
    /// Memory store with region annotation.
    MStore {
        offset: Value,
        value: Value,
        region: MemoryRegion,
    },

    /// Memory store (single byte).
    MStore8 {
        offset: Value,
        value: Value,
        region: MemoryRegion,
    },

    /// Memory copy.
    MCopy {
        dest: Value,
        src: Value,
        length: Value,
    },

    // Storage operations
    /// Storage store with optional static slot.
    SStore {
        key: Value,
        value: Value,
        /// If key is a compile-time constant, store it here for analysis.
        static_slot: Option<BigUint>,
    },

    /// Transient storage store.
    TStore {
        key: Value,
        value: Value,
    },

    // Structured control flow (with explicit value flow)
    /// Structured if with explicit yields.
    If {
        condition: Value,
        /// Input values passed into regions (for SSA).
        inputs: Vec<Value>,
        /// Then region.
        then_region: Region,
        /// Optional else region (defaults to yielding inputs unchanged).
        else_region: Option<Region>,
        /// Output value bindings (SSA values defined by this If).
        outputs: Vec<ValueId>,
    },

    /// Switch statement with explicit yields.
    Switch {
        scrutinee: Value,
        inputs: Vec<Value>,
        cases: Vec<SwitchCase>,
        default: Option<Region>,
        outputs: Vec<ValueId>,
    },

    /// For loop with structured regions and explicit loop-carried values.
    For {
        /// Initial values for loop-carried variables.
        init_values: Vec<Value>,
        /// Loop-carried variable bindings (visible in condition, body, post).
        loop_vars: Vec<ValueId>,
        /// Statements to execute before evaluating condition (evaluated each iteration).
        /// These are generated inside the loop header block.
        condition_stmts: Vec<Statement>,
        /// Condition expression (evaluated each iteration after condition_stmts).
        condition: Expr,
        /// Loop body (yields current values of loop-carried variables).
        body: Region,
        /// Input ValueIds for the post region, one per loop-carried variable.
        /// These receive the body's yielded values (merged with continue-site values
        /// via phi nodes in the LLVM codegen).
        post_input_vars: Vec<ValueId>,
        /// Post-iteration block (yields updated loop vars).
        post: Region,
        /// Final values after loop exits.
        outputs: Vec<ValueId>,
    },

    /// Loop control - break out of the innermost for loop.
    /// Carries the current values of loop-carried variables at the point of break.
    Break {
        /// Current values of loop-carried variables at the break point.
        values: Vec<Value>,
    },
    /// Loop control - continue to the next iteration of the innermost for loop.
    /// Carries the current values of loop-carried variables at the continue point.
    Continue {
        /// Current values of loop-carried variables at the continue point.
        values: Vec<Value>,
    },
    /// Leave the current function, returning the given values.
    Leave {
        /// The current values of the return variables to return.
        return_values: Vec<Value>,
    },

    // Terminating statements
    Revert {
        offset: Value,
        length: Value,
    },
    Return {
        offset: Value,
        length: Value,
    },
    Stop,
    Invalid,
    SelfDestruct {
        address: Value,
    },

    /// Solidity panic revert: emits `Panic(uint256)` ABI encoding and reverts.
    /// Equivalent to: mstore(0, 0x4e487b71...), mstore(4, code), revert(0, 0x24).
    /// Outlined to a shared helper function to avoid duplicating the pattern.
    PanicRevert {
        /// The panic error code (e.g., 0x11 = overflow, 0x22 = encoding, 0x41 = memory).
        code: u8,
    },

    // External calls
    ExternalCall {
        kind: CallKind,
        gas: Value,
        address: Value,
        value: Option<Value>,
        args_offset: Value,
        args_length: Value,
        ret_offset: Value,
        ret_length: Value,
        result: ValueId,
    },

    Create {
        kind: CreateKind,
        value: Value,
        offset: Value,
        length: Value,
        salt: Option<Value>,
        result: ValueId,
    },

    // Logging
    Log {
        offset: Value,
        length: Value,
        topics: Vec<Value>,
    },

    // Data operations
    CodeCopy {
        dest: Value,
        offset: Value,
        length: Value,
    },
    ExtCodeCopy {
        address: Value,
        dest: Value,
        offset: Value,
        length: Value,
    },
    ReturnDataCopy {
        dest: Value,
        offset: Value,
        length: Value,
    },
    DataCopy {
        dest: Value,
        offset: Value,
        length: Value,
    },
    CallDataCopy {
        dest: Value,
        offset: Value,
        length: Value,
    },

    /// Nested block scope.
    Block(Region),

    /// Expression evaluated for side effects only (result discarded).
    Expr(Expr),

    /// Set immutable variable.
    SetImmutable {
        /// The immutable variable identifier.
        key: String,
        /// The value to store.
        value: Value,
    },
}

/// Function definition.
#[derive(Clone, Debug)]
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub params: Vec<(ValueId, Type)>,
    pub returns: Vec<Type>,
    /// Initial SSA value IDs for return variables (allocated at function entry).
    /// These are the IDs that the function body's If statements will reference
    /// as "before" values.
    pub return_values_initial: Vec<ValueId>,
    /// Final SSA value IDs for return variables (after body execution).
    /// These are the values that should be stored to the return pointer.
    pub return_values: Vec<ValueId>,
    pub body: Block,
    /// Number of call sites (for inlining decisions).
    pub call_count: usize,
    /// Instruction count estimate (for inlining decisions).
    pub size_estimate: usize,
}

impl Function {
    /// Creates a new function.
    pub fn new(id: FunctionId, name: String) -> Self {
        Function {
            id,
            name,
            params: Vec::new(),
            returns: Vec::new(),
            return_values_initial: Vec::new(),
            return_values: Vec::new(),
            body: Block::new(),
            call_count: 0,
            size_estimate: 0,
        }
    }
}

/// Top-level object (contract).
#[derive(Clone, Debug)]
pub struct Object {
    pub name: String,
    pub code: Block,
    pub functions: BTreeMap<FunctionId, Function>,
    pub subobjects: Vec<Object>,
    pub data: BTreeMap<String, Vec<u8>>,
}

impl Object {
    /// Creates a new object.
    pub fn new(name: String) -> Self {
        Object {
            name,
            code: Block::new(),
            functions: BTreeMap::new(),
            subobjects: Vec::new(),
            data: BTreeMap::new(),
        }
    }

    /// Counts the total number of heap memory operations (MLoad, MStore, MStore8, MCopy)
    /// in this object including all functions and subobjects.
    /// This is used to estimate the number of `__sbrk_internal` call sites after LLVM codegen.
    pub fn count_heap_operations(&self) -> usize {
        let mut count = count_heap_ops_in_block(&self.code);
        for function in self.functions.values() {
            count += count_heap_ops_in_block(&function.body);
        }
        for subobject in &self.subobjects {
            count += subobject.count_heap_operations();
        }
        count
    }

    /// Counts the total number of exit operations (Return, Revert, Stop) in this object
    /// including all functions and subobjects.
    /// This is used to estimate the number of `__revive_exit` call sites after LLVM codegen.
    pub fn count_exit_operations(&self) -> usize {
        let mut count = count_exit_ops_in_block(&self.code);
        for function in self.functions.values() {
            count += count_exit_ops_in_block(&function.body);
        }
        for subobject in &self.subobjects {
            count += subobject.count_exit_operations();
        }
        count
    }

    /// Counts the total number of `Keccak256Single` expression nodes in this object
    /// (including functions and subobjects).
    /// Used to conditionally emit the `__revive_keccak256_one_word` helper function
    /// only when enough call sites exist to justify the function body cost.
    pub fn count_keccak256_single(&self) -> usize {
        let mut count = count_keccak_single_in_block(&self.code);
        for function in self.functions.values() {
            count += count_keccak_single_in_block(&function.body);
        }
        for subobject in &self.subobjects {
            count += subobject.count_keccak256_single();
        }
        count
    }
}

/// Counts the occurrences of callvalue and calldataload expressions.
///
/// Returns `(callvalue_count, calldataload_count)`.
/// Used to decide whether outlining these into shared functions saves code.
#[derive(Debug, Default, Clone, Copy)]
pub struct SyscallCounts {
    /// Number of `callvalue()` expression sites.
    pub callvalue: usize,
    /// Number of `calldataload(offset)` expression sites.
    pub calldataload: usize,
    /// Number of `caller()` expression sites.
    pub caller: usize,
    /// Number of heap memory operations (MStore, MLoad, MStore8, MCopy, etc.)
    /// that translate to sbrk calls at the LLVM level.
    pub heap_operations: usize,
}

impl std::ops::AddAssign for SyscallCounts {
    fn add_assign(&mut self, rhs: Self) {
        self.callvalue += rhs.callvalue;
        self.calldataload += rhs.calldataload;
        self.caller += rhs.caller;
        self.heap_operations += rhs.heap_operations;
    }
}

impl Object {
    /// Counts the total number of callvalue and calldataload expression sites
    /// in this object including all functions and subobjects.
    pub fn count_syscall_sites(&self) -> SyscallCounts {
        let mut counts = count_syscalls_in_block(&self.code);
        for function in self.functions.values() {
            counts += count_syscalls_in_block(&function.body);
        }
        for subobject in &self.subobjects {
            counts += subobject.count_syscall_sites();
        }
        counts
    }
}

fn count_syscalls_in_block(block: &Block) -> SyscallCounts {
    let mut counts = SyscallCounts::default();
    for stmt in &block.statements {
        counts += count_syscalls_in_statement(stmt);
    }
    counts
}

fn count_syscalls_in_statement(stmt: &Statement) -> SyscallCounts {
    match stmt {
        Statement::Let { value, .. } => count_syscalls_in_expr(value),
        Statement::MStore { .. } | Statement::MStore8 { .. } | Statement::MCopy { .. } => {
            SyscallCounts {
                heap_operations: 1,
                ..Default::default()
            }
        }
        Statement::Revert { .. }
        | Statement::Return { .. }
        | Statement::CodeCopy { .. }
        | Statement::ExtCodeCopy { .. }
        | Statement::ReturnDataCopy { .. }
        | Statement::DataCopy { .. }
        | Statement::CallDataCopy { .. } => SyscallCounts {
            heap_operations: 1,
            ..Default::default()
        },
        Statement::ExternalCall { .. } | Statement::Create { .. } => SyscallCounts {
            heap_operations: 2,
            ..Default::default()
        },
        Statement::Log { topics, .. } => SyscallCounts {
            heap_operations: 1 + topics.len(),
            ..Default::default()
        },
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            let mut n = count_syscalls_in_region(then_region);
            if let Some(r) = else_region {
                n += count_syscalls_in_region(r);
            }
            n
        }
        Statement::Switch { cases, default, .. } => {
            let mut n = SyscallCounts::default();
            for case in cases {
                n += count_syscalls_in_region(&case.body);
            }
            if let Some(r) = default {
                n += count_syscalls_in_region(r);
            }
            n
        }
        Statement::For {
            condition_stmts,
            body,
            post,
            ..
        } => {
            let mut n = SyscallCounts::default();
            for s in condition_stmts {
                n += count_syscalls_in_statement(s);
            }
            n += count_syscalls_in_region(body);
            n += count_syscalls_in_region(post);
            n
        }
        Statement::Block(region) => count_syscalls_in_region(region),
        _ => SyscallCounts::default(),
    }
}

fn count_syscalls_in_expr(expr: &Expr) -> SyscallCounts {
    match expr {
        Expr::CallValue => SyscallCounts {
            callvalue: 1,
            ..Default::default()
        },
        Expr::CallDataLoad { .. } => SyscallCounts {
            calldataload: 1,
            ..Default::default()
        },
        Expr::Caller => SyscallCounts {
            caller: 1,
            ..Default::default()
        },
        Expr::MLoad { .. } | Expr::Keccak256 { .. } => SyscallCounts {
            heap_operations: 1,
            ..Default::default()
        },
        _ => SyscallCounts::default(),
    }
}

fn count_syscalls_in_region(region: &Region) -> SyscallCounts {
    let mut counts = SyscallCounts::default();
    for stmt in &region.statements {
        counts += count_syscalls_in_statement(stmt);
    }
    counts
}

/// Counts heap memory operations in a block recursively.
fn count_heap_ops_in_block(block: &Block) -> usize {
    let mut count = 0;
    for stmt in &block.statements {
        count += count_heap_ops_in_statement(stmt);
    }
    count
}

/// Counts heap memory operations in a single statement recursively.
fn count_heap_ops_in_statement(stmt: &Statement) -> usize {
    match stmt {
        Statement::MStore { .. } | Statement::MStore8 { .. } | Statement::MCopy { .. } => 1,
        Statement::Let { value, .. } => count_heap_ops_in_expr(value),
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            let mut n = count_heap_ops_in_region(then_region);
            if let Some(r) = else_region {
                n += count_heap_ops_in_region(r);
            }
            n
        }
        Statement::Switch { cases, default, .. } => {
            let mut n = 0;
            for case in cases {
                n += count_heap_ops_in_region(&case.body);
            }
            if let Some(r) = default {
                n += count_heap_ops_in_region(r);
            }
            n
        }
        Statement::For {
            condition_stmts,
            body,
            post,
            ..
        } => {
            let mut n = 0;
            for s in condition_stmts {
                n += count_heap_ops_in_statement(s);
            }
            n += count_heap_ops_in_region(body);
            n += count_heap_ops_in_region(post);
            n
        }
        Statement::Block(region) => count_heap_ops_in_region(region),
        _ => 0,
    }
}

/// Counts heap memory operations in an expression (only MLoad).
fn count_heap_ops_in_expr(expr: &Expr) -> usize {
    match expr {
        Expr::MLoad { .. } => 1,
        _ => 0,
    }
}

/// Counts heap memory operations in a region.
fn count_heap_ops_in_region(region: &Region) -> usize {
    let mut count = 0;
    for stmt in &region.statements {
        count += count_heap_ops_in_statement(stmt);
    }
    count
}

/// Counts exit operations (Return, Revert, Stop) in a block recursively.
fn count_exit_ops_in_block(block: &Block) -> usize {
    let mut count = 0;
    for stmt in &block.statements {
        count += count_exit_ops_in_statement(stmt);
    }
    count
}

/// Counts exit operations in a single statement recursively.
fn count_exit_ops_in_statement(stmt: &Statement) -> usize {
    match stmt {
        Statement::Return { .. } | Statement::Revert { .. } | Statement::Stop => 1,
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            let mut n = count_exit_ops_in_region(then_region);
            if let Some(r) = else_region {
                n += count_exit_ops_in_region(r);
            }
            n
        }
        Statement::Switch { cases, default, .. } => {
            let mut n = 0;
            for case in cases {
                n += count_exit_ops_in_region(&case.body);
            }
            if let Some(r) = default {
                n += count_exit_ops_in_region(r);
            }
            n
        }
        Statement::For {
            condition_stmts,
            body,
            post,
            ..
        } => {
            let mut n = 0;
            for s in condition_stmts {
                n += count_exit_ops_in_statement(s);
            }
            n += count_exit_ops_in_region(body);
            n += count_exit_ops_in_region(post);
            n
        }
        Statement::Block(region) => count_exit_ops_in_region(region),
        _ => 0,
    }
}

/// Counts exit operations in a region.
fn count_exit_ops_in_region(region: &Region) -> usize {
    let mut count = 0;
    for stmt in &region.statements {
        count += count_exit_ops_in_statement(stmt);
    }
    count
}

/// Counts `Keccak256Single` expression nodes in a block.
fn count_keccak_single_in_block(block: &Block) -> usize {
    block
        .statements
        .iter()
        .map(count_keccak_single_in_statement)
        .sum()
}

/// Counts `Keccak256Single` expression nodes in a statement (recursively).
fn count_keccak_single_in_statement(stmt: &Statement) -> usize {
    match stmt {
        Statement::Let { value, .. } | Statement::Expr(value) => {
            if matches!(value, Expr::Keccak256Single { .. }) {
                1
            } else {
                0
            }
        }
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            count_keccak_single_in_region(then_region)
                + else_region
                    .as_ref()
                    .map_or(0, count_keccak_single_in_region)
        }
        Statement::Switch { cases, default, .. } => {
            cases
                .iter()
                .map(|c| count_keccak_single_in_region(&c.body))
                .sum::<usize>()
                + default.as_ref().map_or(0, count_keccak_single_in_region)
        }
        Statement::For {
            condition_stmts,
            body,
            post,
            ..
        } => {
            condition_stmts
                .iter()
                .map(count_keccak_single_in_statement)
                .sum::<usize>()
                + count_keccak_single_in_region(body)
                + count_keccak_single_in_region(post)
        }
        Statement::Block(region) => count_keccak_single_in_region(region),
        _ => 0,
    }
}

/// Counts `Keccak256Single` expression nodes in a region.
fn count_keccak_single_in_region(region: &Region) -> usize {
    region
        .statements
        .iter()
        .map(count_keccak_single_in_statement)
        .sum()
}
