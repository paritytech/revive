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

    /// Returns the smallest BitWidth variant that has at least `bits` bits.
    pub fn from_bits(bits: u32) -> Self {
        if bits <= 1 {
            BitWidth::I1
        } else if bits <= 8 {
            BitWidth::I8
        } else if bits <= 32 {
            BitWidth::I32
        } else if bits <= 64 {
            BitWidth::I64
        } else if bits <= 160 {
            BitWidth::I160
        } else {
            BitWidth::I256
        }
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

    /// Compound mapping load: keccak256(key, slot) → sload.
    /// Combines a Keccak256Pair hash with a storage load into one outlined call.
    /// Only valid when the hash intermediate is used exclusively by this load.
    MappingSLoad {
        /// The mapping key (first word of keccak256 input).
        key: Value,
        /// The storage slot (second word of keccak256 input).
        slot: Value,
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

    /// Error string revert: emits `Error(string)` ABI encoding and reverts.
    /// Equivalent to: mload(0x40) → mstore(fmp, selector) → mstore(fmp+4, 0x20) →
    /// mstore(fmp+0x24, length) → mstore(fmp+0x44, word0) → [...] → revert(fmp, total).
    /// Outlined to a shared helper function parameterized by string length and data words.
    ErrorStringRevert {
        /// The string length in bytes.
        length: u8,
        /// The string data words (1-4 words of 32 bytes each).
        data: Vec<BigUint>,
    },

    /// Custom error revert: emits a custom error revert using scratch space.
    /// Pattern: mstore(0, selector) + [mstore(4, arg0) + mstore(0x24, arg1) + ...] + revert(0, 4+32*N).
    /// Uses scratch space (offset 0), so no FMP load needed.
    CustomErrorRevert {
        /// The 4-byte error selector, left-shifted by 224 bits (as stored in scratch).
        selector: BigUint,
        /// The arguments to the custom error (0-3 values).
        args: Vec<Value>,
    },

    /// Compound mapping store: keccak256(key, slot) → sstore(hash, value).
    /// Combines a Keccak256Pair hash with a storage store into one outlined call.
    /// Only valid when the hash intermediate is used exclusively by this store.
    MappingSStore {
        /// The mapping key (first word of keccak256 input).
        key: Value,
        /// The storage slot (second word of keccak256 input).
        slot: Value,
        /// The value to store.
        value: Value,
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

    /// Counts the total number of `ErrorStringRevert` statements grouped by
    /// number of data words. Returns a map from num_words → count.
    /// Used to conditionally outline: only profitable with >= 2 call sites.
    pub fn count_error_string_reverts(&self) -> BTreeMap<usize, usize> {
        let mut counts = BTreeMap::new();
        count_error_string_reverts_in_block(&self.code, &mut counts);
        for function in self.functions.values() {
            count_error_string_reverts_in_block(&function.body, &mut counts);
        }
        // Don't count subobjects - each subobject has its own outlined functions
        counts
    }

    /// Counts the total number of `CustomErrorRevert` statements grouped by
    /// num_args. Returns a map from num_args → count.
    /// Used to conditionally outline: only profitable with >= 3 call sites.
    pub fn count_custom_error_reverts(&self) -> BTreeMap<usize, usize> {
        let mut counts = BTreeMap::new();
        count_custom_error_reverts_in_block(&self.code, &mut counts);
        for function in self.functions.values() {
            count_custom_error_reverts_in_block(&function.body, &mut counts);
        }
        counts
    }

    /// Returns true if any code in this object uses the `msize()` expression.
    /// When false, the msize watermark (`GLOBAL_HEAP_SIZE`) doesn't need updating,
    /// allowing InlineNative stores to skip the `ensure_heap_size` call.
    pub fn has_msize(&self) -> bool {
        has_msize_in_block(&self.code)
            || self.functions.values().any(|f| has_msize_in_block(&f.body))
            || self.subobjects.iter().any(|s| s.has_msize())
    }

    /// Finds the maximum ValueId used anywhere in this object (code + functions).
    /// Does NOT recurse into subobjects.
    pub fn find_max_value_id(&self) -> u32 {
        let mut max_id: u32 = 0;

        fn update(id: u32, max: &mut u32) {
            *max = (*max).max(id);
        }

        fn scan_expr(expr: &Expr, m: &mut u32) {
            match expr {
                Expr::Var(id) => update(id.0, m),
                Expr::Binary { lhs, rhs, .. } => {
                    update(lhs.id.0, m);
                    update(rhs.id.0, m);
                }
                Expr::Ternary { a, b, n, .. } => {
                    update(a.id.0, m);
                    update(b.id.0, m);
                    update(n.id.0, m);
                }
                Expr::Unary { operand, .. } => update(operand.id.0, m),
                Expr::CallDataLoad { offset } => update(offset.id.0, m),
                Expr::ExtCodeSize { address }
                | Expr::ExtCodeHash { address }
                | Expr::Balance { address } => update(address.id.0, m),
                Expr::BlockHash { number } => update(number.id.0, m),
                Expr::BlobHash { index } => update(index.id.0, m),
                Expr::MLoad { offset, .. } => update(offset.id.0, m),
                Expr::SLoad { key, .. } | Expr::TLoad { key } => update(key.id.0, m),
                Expr::Call { args, .. } => {
                    for a in args {
                        update(a.id.0, m);
                    }
                }
                Expr::Truncate { value, .. }
                | Expr::ZeroExtend { value, .. }
                | Expr::SignExtendTo { value, .. } => update(value.id.0, m),
                Expr::Keccak256 { offset, length } => {
                    update(offset.id.0, m);
                    update(length.id.0, m);
                }
                Expr::Keccak256Pair { word0, word1 }
                | Expr::MappingSLoad {
                    key: word0,
                    slot: word1,
                } => {
                    update(word0.id.0, m);
                    update(word1.id.0, m);
                }
                Expr::Keccak256Single { word0 } => update(word0.id.0, m),
                _ => {}
            }
        }

        fn scan_stmt(stmt: &Statement, m: &mut u32) {
            match stmt {
                Statement::Let { bindings, value } => {
                    for b in bindings {
                        update(b.0, m);
                    }
                    scan_expr(value, m);
                }
                Statement::MStore { offset, value, .. }
                | Statement::MStore8 { offset, value, .. } => {
                    update(offset.id.0, m);
                    update(value.id.0, m);
                }
                Statement::MCopy { dest, src, length } => {
                    update(dest.id.0, m);
                    update(src.id.0, m);
                    update(length.id.0, m);
                }
                Statement::SStore { key, value, .. } | Statement::TStore { key, value } => {
                    update(key.id.0, m);
                    update(value.id.0, m);
                }
                Statement::MappingSStore { key, slot, value } => {
                    update(key.id.0, m);
                    update(slot.id.0, m);
                    update(value.id.0, m);
                }
                Statement::If {
                    condition,
                    inputs,
                    then_region,
                    else_region,
                    outputs,
                } => {
                    update(condition.id.0, m);
                    for v in inputs {
                        update(v.id.0, m);
                    }
                    scan_region(then_region, m);
                    if let Some(r) = else_region {
                        scan_region(r, m);
                    }
                    for o in outputs {
                        update(o.0, m);
                    }
                }
                Statement::Switch {
                    scrutinee,
                    inputs,
                    cases,
                    default,
                    outputs,
                } => {
                    update(scrutinee.id.0, m);
                    for v in inputs {
                        update(v.id.0, m);
                    }
                    for c in cases {
                        scan_region(&c.body, m);
                    }
                    if let Some(r) = default {
                        scan_region(r, m);
                    }
                    for o in outputs {
                        update(o.0, m);
                    }
                }
                Statement::For {
                    init_values,
                    loop_vars,
                    condition_stmts,
                    condition,
                    body,
                    post_input_vars,
                    post,
                    outputs,
                } => {
                    for v in init_values {
                        update(v.id.0, m);
                    }
                    for v in loop_vars {
                        update(v.0, m);
                    }
                    for s in condition_stmts {
                        scan_stmt(s, m);
                    }
                    scan_expr(condition, m);
                    scan_region(body, m);
                    for v in post_input_vars {
                        update(v.0, m);
                    }
                    scan_region(post, m);
                    for o in outputs {
                        update(o.0, m);
                    }
                }
                Statement::Leave { return_values }
                | Statement::Break {
                    values: return_values,
                }
                | Statement::Continue {
                    values: return_values,
                } => {
                    for v in return_values {
                        update(v.id.0, m);
                    }
                }
                Statement::Revert { offset, length } | Statement::Return { offset, length } => {
                    update(offset.id.0, m);
                    update(length.id.0, m);
                }
                Statement::SelfDestruct { address } => update(address.id.0, m),
                Statement::ExternalCall {
                    gas,
                    address,
                    value,
                    args_offset,
                    args_length,
                    ret_offset,
                    ret_length,
                    result,
                    ..
                } => {
                    update(gas.id.0, m);
                    update(address.id.0, m);
                    if let Some(v) = value {
                        update(v.id.0, m);
                    }
                    update(args_offset.id.0, m);
                    update(args_length.id.0, m);
                    update(ret_offset.id.0, m);
                    update(ret_length.id.0, m);
                    update(result.0, m);
                }
                Statement::Create {
                    value,
                    offset,
                    length,
                    salt,
                    result,
                    ..
                } => {
                    update(value.id.0, m);
                    update(offset.id.0, m);
                    update(length.id.0, m);
                    if let Some(s) = salt {
                        update(s.id.0, m);
                    }
                    update(result.0, m);
                }
                Statement::Log {
                    offset,
                    length,
                    topics,
                } => {
                    update(offset.id.0, m);
                    update(length.id.0, m);
                    for t in topics {
                        update(t.id.0, m);
                    }
                }
                Statement::CodeCopy {
                    dest,
                    offset,
                    length,
                }
                | Statement::ReturnDataCopy {
                    dest,
                    offset,
                    length,
                }
                | Statement::DataCopy {
                    dest,
                    offset,
                    length,
                }
                | Statement::CallDataCopy {
                    dest,
                    offset,
                    length,
                } => {
                    update(dest.id.0, m);
                    update(offset.id.0, m);
                    update(length.id.0, m);
                }
                Statement::ExtCodeCopy {
                    address,
                    dest,
                    offset,
                    length,
                } => {
                    update(address.id.0, m);
                    update(dest.id.0, m);
                    update(offset.id.0, m);
                    update(length.id.0, m);
                }
                Statement::Block(region) => scan_region(region, m),
                Statement::Expr(expr) => scan_expr(expr, m),
                Statement::SetImmutable { value, .. } => update(value.id.0, m),
                Statement::CustomErrorRevert { args, .. } => {
                    for a in args {
                        update(a.id.0, m);
                    }
                }
                Statement::Stop
                | Statement::Invalid
                | Statement::PanicRevert { .. }
                | Statement::ErrorStringRevert { .. } => {}
            }
        }

        fn scan_region(region: &Region, m: &mut u32) {
            for stmt in &region.statements {
                scan_stmt(stmt, m);
            }
            for v in &region.yields {
                update(v.id.0, m);
            }
        }

        fn scan_block(block: &Block, m: &mut u32) {
            for stmt in &block.statements {
                scan_stmt(stmt, m);
            }
        }

        scan_block(&self.code, &mut max_id);
        for function in self.functions.values() {
            for (param_id, _) in &function.params {
                update(param_id.0, &mut max_id);
            }
            for id in &function.return_values_initial {
                update(id.0, &mut max_id);
            }
            for id in &function.return_values {
                update(id.0, &mut max_id);
            }
            scan_block(&function.body, &mut max_id);
        }
        for sub in &self.subobjects {
            max_id = max_id.max(sub.find_max_value_id());
        }

        max_id
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

/// Visits every statement in a slice recursively, calling `f` on each one.
/// Handles structural recursion into If/Switch/For/Block regions.
pub fn for_each_stmt(stmts: &[Statement], f: &mut dyn FnMut(&Statement)) {
    for stmt in stmts {
        f(stmt);
        match stmt {
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                for_each_stmt(&then_region.statements, f);
                if let Some(r) = else_region {
                    for_each_stmt(&r.statements, f);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    for_each_stmt(&case.body.statements, f);
                }
                if let Some(r) = default {
                    for_each_stmt(&r.statements, f);
                }
            }
            Statement::For {
                condition_stmts,
                body,
                post,
                ..
            } => {
                for_each_stmt(condition_stmts, f);
                for_each_stmt(&body.statements, f);
                for_each_stmt(&post.statements, f);
            }
            Statement::Block(region) => for_each_stmt(&region.statements, f),
            _ => {}
        }
    }
}

fn count_syscalls_in_block(block: &Block) -> SyscallCounts {
    let mut counts = SyscallCounts::default();
    for_each_stmt(&block.statements, &mut |stmt| match stmt {
        Statement::Let { value, .. } | Statement::Expr(value) => match value {
            Expr::CallValue => counts.callvalue += 1,
            Expr::CallDataLoad { .. } => counts.calldataload += 1,
            Expr::Caller => counts.caller += 1,
            Expr::MLoad { .. } | Expr::Keccak256 { .. } => counts.heap_operations += 1,
            _ => {}
        },
        Statement::MStore { .. } | Statement::MStore8 { .. } | Statement::MCopy { .. } => {
            counts.heap_operations += 1;
        }
        Statement::Revert { .. }
        | Statement::Return { .. }
        | Statement::CodeCopy { .. }
        | Statement::ExtCodeCopy { .. }
        | Statement::ReturnDataCopy { .. }
        | Statement::DataCopy { .. }
        | Statement::CallDataCopy { .. } => {
            counts.heap_operations += 1;
        }
        Statement::ExternalCall { .. } | Statement::Create { .. } => {
            counts.heap_operations += 2;
        }
        Statement::Log { topics, .. } => {
            counts.heap_operations += 1 + topics.len();
        }
        _ => {}
    });
    counts
}

fn count_heap_ops_in_block(block: &Block) -> usize {
    let mut count = 0usize;
    for_each_stmt(&block.statements, &mut |stmt| match stmt {
        Statement::MStore { .. } | Statement::MStore8 { .. } | Statement::MCopy { .. } => {
            count += 1;
        }
        Statement::Let {
            value: Expr::MLoad { .. },
            ..
        } => count += 1,
        _ => {}
    });
    count
}

fn count_exit_ops_in_block(block: &Block) -> usize {
    let mut count = 0usize;
    for_each_stmt(&block.statements, &mut |stmt| {
        if matches!(
            stmt,
            Statement::Return { .. } | Statement::Revert { .. } | Statement::Stop
        ) {
            count += 1;
        }
    });
    count
}

fn count_keccak_single_in_block(block: &Block) -> usize {
    let mut count = 0usize;
    for_each_stmt(&block.statements, &mut |stmt| {
        if let Statement::Let { value, .. } | Statement::Expr(value) = stmt {
            if matches!(value, Expr::Keccak256Single { .. }) {
                count += 1;
            }
        }
    });
    count
}

fn count_error_string_reverts_in_block(block: &Block, counts: &mut BTreeMap<usize, usize>) {
    for_each_stmt(&block.statements, &mut |stmt| {
        if let Statement::ErrorStringRevert { data, .. } = stmt {
            *counts.entry(data.len()).or_insert(0) += 1;
        }
    });
}

fn count_custom_error_reverts_in_block(block: &Block, counts: &mut BTreeMap<usize, usize>) {
    for_each_stmt(&block.statements, &mut |stmt| {
        if let Statement::CustomErrorRevert { args, .. } = stmt {
            *counts.entry(args.len()).or_insert(0) += 1;
        }
    });
}

fn has_msize_in_block(block: &Block) -> bool {
    let mut found = false;
    for_each_stmt(&block.statements, &mut |stmt| {
        if let Statement::Let { value, .. } | Statement::Expr(value) = stmt {
            if matches!(value, Expr::MSize) {
                found = true;
            }
        }
    });
    found
}
