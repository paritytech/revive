# Revive IR Implementation Plan

## Overview

This document outlines the plan to introduce a new intermediate representation (IR) between Yul and LLVM IR. The goal is to enable custom optimizations that LLVM cannot perform because it lacks the semantic knowledge of our target (PolkaVM) and source (EVM/Solidity) domains.

> **Implementation Status**: The `newyork` crate (`crates/newyork/`) implements this IR. See `newyork_status.md` for current progress.

```
Current:  Yul AST ────────────────────────────────► LLVM IR ──► RISC-V
                        (llvm-context)

New:      Yul AST ──► Revive IR ──► [Optimizations] ──► LLVM IR ──► RISC-V
                   (visitor)     (custom passes)     (codegen)
```

## Why a New IR?

### The Core Problem

From COMPILER.md: *"The YUL IR emitted by solc is heavily optimized for the EVM. Because RISC-V and EVM are orthogonal target machines, this does not work well for us."*

Key mismatches:
- **Word size**: EVM is 256-bit, RISC-V is 64-bit
- **Endianness**: EVM is big-endian, RISC-V is little-endian
- **Architecture**: EVM is a stack machine, RISC-V is register-based
- **Memory model**: EVM has linear memory with a "free pointer" convention

LLVM is powerful but operates at too low a level to understand these semantic patterns. By the time Yul reaches LLVM, the high-level intent is lost.

### What the IR Enables

1. **Type Inference**: Narrow 256-bit operations to 64/32/8-bit where provable
2. **Custom Inlining**: Make smarter decisions than LLVM's generic heuristics
3. **Memory Optimization**: Recognize EVM memory patterns and eliminate redundancy
4. **Semantic Rewrites**: Transform EVM idioms into efficient RISC-V equivalents

---

## Implementation Learnings

> **Note**: This section documents discoveries made during implementation that weren't anticipated in the original plan.

### SSA Construction Complexities

1. **For loop conditions need statements**: Yul conditions can contain statements (e.g., `let x := ...` inside the condition block). These must execute *inside* the loop header on each iteration, not before the loop. The IR needs:
   ```rust
   For {
       condition_stmts: Vec<Statement>,  // Execute before evaluating condition
       condition: Expr,
       ...
   }
   ```

2. **Function return values need dual tracking**: Functions need both initial SSA IDs (at entry) AND final SSA IDs (after body) to properly handle `leave` statements:
   ```rust
   Function {
       return_values_initial: Vec<ValueId>,  // IDs at function entry
       return_values: Vec<ValueId>,          // Final IDs after body
       ...
   }
   ```

3. **For loop body and post share scope**: Modifications in the loop body must be visible in the post block. They share a scope during translation, with the combined modifications fed back through phi nodes.

4. **Leave statement carries return values**: The `leave` statement must capture current SSA values of return variables:
   ```rust
   Leave { return_values: Vec<Value> }
   ```

### Translation is Two-Pass

Functions require two-pass translation due to potential mutual recursion:
1. **First pass** (`collect_functions`): Pre-allocate function IDs and parameter ValueIds
2. **Second pass** (`translate_function_definition`): Translate bodies using pre-allocated IDs

### Additional Constructs Discovered

**Ternary operations**: `addmod` and `mulmod` are ternary (3 operands), not binary:
```rust
Expr::Ternary { op: BinOp, a: Value, b: Value, n: Value }
```

**Additional EVM builtins not in original plan**:
- `Balance { address }` - get balance of an address
- `DataOffset { id }` / `DataSize { id }` - for deployed bytecode
- `LoadImmutable { key }` / `SetImmutable { key, value }` - immutable variables
- `LinkerSymbol { path }` - external library addresses
- `CallDataCopy` - distinct from generic DataCopy
- `UnaryOp::Clz` - count leading zeros

**String literal handling**: Must be right-padded to 32 bytes to match Yul pipeline:
```rust
let mut padded = vec![0u8; 32];
let len = bytes.len().min(32);
padded[..len].copy_from_slice(&bytes[..len]);
```

### LLVM Lowering Complexities

The "simple" MLIR SCF-style structured control flow requires significant machinery to lower to LLVM's CFG-based IR:

1. **Terminator tracking**: LLVM basic blocks need proper terminators; must track whether blocks are "terminated"
2. **Loop control flow**: Break/continue require explicit tracking of `continue_block` and `break_block` targets
3. **Phi builder state**: Complex state management for building phi nodes at control flow joins
4. **External call variants**: Different call kinds (Call, CallCode, DelegateCall, StaticCall) have different argument counts and semantics

---

## IR Design

### Design Principles

1. **SSA with Structured Control Flow** (like MLIR's SCF dialect)
   - Preserves high-level structure from Yul
   - Values flow through explicit region arguments and yields
   - Easier to analyze and transform than CFG with phi nodes

2. **Explicit Types with Address Spaces**
   - Every value has a known bit-width
   - Pointers carry address space information (Heap, Stack, Storage)
   - Initially all `I256`, narrowed by type inference
   - Explicit conversion operations (truncate, extend)

3. **Pure Expressions vs Effectful Statements**
   - Expressions compute values without side effects
   - Statements perform effects (memory, storage, control flow)
   - Enables easier reasoning about rewrites

4. **Semantic Annotations**
   - Storage operations tagged with slot IDs when statically known
   - Memory operations tagged with region information
   - Enables domain-specific optimizations

### IR Structure

```rust
//=============================================================================
// Types
//=============================================================================

/// Bit width for integer types
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum BitWidth {
    I1   = 1,
    I8   = 8,
    I32  = 32,
    I64  = 64,
    I160 = 160,
    I256 = 256,
}

/// Address space for pointers - distinguishes memory regions
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AddressSpace {
    /// EVM heap memory (linear, big-endian)
    Heap,
    /// Native stack allocations (little-endian, optimizable)
    Stack,
    /// Contract storage (key-value, 256-bit slots)
    Storage,
    /// Code/data segment (read-only)
    Code,
}

/// Type of a value in the IR
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    /// Integer with specific bit width
    Int(BitWidth),
    /// Pointer with address space
    Ptr(AddressSpace),
    /// No value (for statements)
    Void,
}

/// Memory region annotation for heap operations
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MemoryRegion {
    /// Scratch space: addresses 0x00-0x3f (64 bytes)
    Scratch,
    /// Free memory pointer location: address 0x40
    FreePointerSlot,
    /// Dynamic allocation region: 0x80+
    Dynamic,
    /// Unknown region (conservative)
    Unknown,
}

//=============================================================================
// SSA Values
//=============================================================================

/// An SSA value reference (index into value table)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ValueId(pub u32);

/// A typed SSA value
#[derive(Clone, Copy, Debug)]
pub struct Value {
    pub id: ValueId,
    pub ty: Type,
}

//=============================================================================
// Expressions (Pure - no side effects)
//=============================================================================

/// Binary operation kinds
#[derive(Clone, Copy, Debug)]
pub enum BinOp {
    // Arithmetic
    Add, Sub, Mul, Div, SDiv, Mod, SMod, Exp,
    // Ternary arithmetic (used with Expr::Ternary, not Expr::Binary)
    AddMod, MulMod,
    // Bitwise
    And, Or, Xor, Shl, Shr, Sar,
    // Comparison (result is I1)
    Lt, Gt, Slt, Sgt, Eq,
    // Byte operations
    Byte, SignExtend,
}

/// Unary operation kinds
#[derive(Clone, Copy, Debug)]
pub enum UnaryOp {
    IsZero,  // result is I1
    Not,
    Clz,     // count leading zeros (discovered during implementation)
}

/// Pure expressions that produce values
#[derive(Clone, Debug)]
pub enum Expr {
    /// Literal constant
    Literal { value: BigUint, ty: Type },

    /// Reference to an SSA value
    Var(ValueId),

    /// Binary operation
    Binary { op: BinOp, lhs: Value, rhs: Value },

    /// Ternary operation (addmod, mulmod) - discovered during implementation
    Ternary { op: BinOp, a: Value, b: Value, n: Value },

    /// Unary operation
    Unary { op: UnaryOp, operand: Value },

    //-------------------------------------------------------------------------
    // EVM Builtins (pure getters)
    //-------------------------------------------------------------------------
    CallDataLoad { offset: Value },
    CallValue,
    Caller,
    Origin,
    CallDataSize,
    CodeSize,
    GasPrice,
    ExtCodeSize { address: Value },
    ReturnDataSize,
    ExtCodeHash { address: Value },
    BlockHash { number: Value },
    Coinbase,
    Timestamp,
    Number,
    Difficulty,
    GasLimit,
    ChainId,
    SelfBalance,
    BaseFee,
    BlobHash { index: Value },
    BlobBaseFee,
    Gas,
    MSize,
    Address,
    Balance { address: Value },  // Added: get balance of address

    //-------------------------------------------------------------------------
    // Memory/Storage Loads
    //-------------------------------------------------------------------------
    /// Memory load with region annotation
    MLoad {
        offset: Value,
        region: MemoryRegion,
    },

    /// Storage load with optional static slot
    SLoad {
        key: Value,
        /// If key is a compile-time constant, store it here for analysis
        static_slot: Option<BigUint>,
    },

    /// Transient storage load
    TLoad { key: Value },

    //-------------------------------------------------------------------------
    // Function call
    //-------------------------------------------------------------------------
    Call { function: FunctionId, args: Vec<Value> },

    //-------------------------------------------------------------------------
    // Type conversions (explicit)
    //-------------------------------------------------------------------------
    Truncate { value: Value, to: BitWidth },
    ZeroExtend { value: Value, to: BitWidth },
    SignExtendTo { value: Value, to: BitWidth },

    //-------------------------------------------------------------------------
    // Keccak256 (pure but expensive)
    //-------------------------------------------------------------------------
    Keccak256 { offset: Value, length: Value },

    //-------------------------------------------------------------------------
    // Additional builtins (discovered during implementation)
    //-------------------------------------------------------------------------
    /// Data offset for deployed bytecode
    DataOffset { id: String },
    /// Data size for deployed bytecode
    DataSize { id: String },
    /// Load immutable variable
    LoadImmutable { key: String },
    /// Linker symbol - returns address of external library
    LinkerSymbol { path: String },
}

//=============================================================================
// Statements (Effectful)
//=============================================================================

/// Statements with effects and structured control flow
#[derive(Clone, Debug)]
pub enum Statement {
    //-------------------------------------------------------------------------
    // SSA Binding
    //-------------------------------------------------------------------------
    /// SSA binding: let x, y, z = expr
    Let {
        bindings: Vec<ValueId>,
        value: Expr,
    },

    //-------------------------------------------------------------------------
    // Memory Operations
    //-------------------------------------------------------------------------
    /// Memory store with region annotation
    MStore {
        offset: Value,
        value: Value,
        region: MemoryRegion,
    },

    /// Memory store (single byte)
    MStore8 {
        offset: Value,
        value: Value,
        region: MemoryRegion,
    },

    /// Memory copy
    MCopy {
        dest: Value,
        src: Value,
        length: Value,
    },

    //-------------------------------------------------------------------------
    // Storage Operations
    //-------------------------------------------------------------------------
    /// Storage store with optional static slot
    SStore {
        key: Value,
        value: Value,
        /// If key is a compile-time constant, store it here for analysis
        static_slot: Option<BigUint>,
    },

    /// Transient storage store
    TStore { key: Value, value: Value },

    //-------------------------------------------------------------------------
    // Structured Control Flow (with explicit value flow)
    //-------------------------------------------------------------------------

    /// Structured if with explicit yields
    ///
    /// Yul has no else, but the IR supports it for optimization.
    /// Values modified in the then-block must be explicitly yielded.
    ///
    /// Example: `if cond { x := 1 }` where x was defined outside
    /// becomes: `x' = If(cond, [x], { yield [1] }, { yield [x] })`
    If {
        condition: Value,
        /// Input values passed into regions (for SSA)
        inputs: Vec<Value>,
        /// Then region
        then_region: Region,
        /// Optional else region (defaults to yielding inputs unchanged)
        else_region: Option<Region>,
        /// Output value bindings (SSA values defined by this If)
        outputs: Vec<ValueId>,
    },

    /// Switch statement with explicit yields
    Switch {
        scrutinee: Value,
        inputs: Vec<Value>,
        cases: Vec<SwitchCase>,
        default: Option<Region>,
        outputs: Vec<ValueId>,
    },

    /// For loop with structured regions and explicit loop-carried values
    ///
    /// Loop-carried variables are passed as region arguments.
    /// The post block yields the updated values for the next iteration.
    ///
    /// **Implementation note**: Body and post regions share a scope during
    /// translation, so modifications in the body are visible in post.
    For {
        /// Initial values for loop-carried variables
        init_values: Vec<Value>,
        /// Loop-carried variable bindings (visible in condition, body, post)
        loop_vars: Vec<ValueId>,
        /// Statements to execute before evaluating condition (each iteration)
        /// **Added during implementation**: Yul conditions can contain statements
        condition_stmts: Vec<Statement>,
        /// Condition expression (evaluated each iteration after condition_stmts)
        condition: Expr,
        /// Loop body
        body: Region,
        /// Post-iteration block (yields updated loop vars)
        post: Region,
        /// Final values after loop exits
        outputs: Vec<ValueId>,
    },

    /// Loop control
    Break,
    Continue,
    /// Leave the current function, returning the given values
    /// **Updated**: Must carry current SSA values of return variables
    Leave { return_values: Vec<Value> },

    //-------------------------------------------------------------------------
    // Terminating Statements
    //-------------------------------------------------------------------------
    Revert { offset: Value, length: Value },
    Return { offset: Value, length: Value },
    Stop,
    Invalid,
    SelfDestruct { address: Value },

    //-------------------------------------------------------------------------
    // External Calls
    //-------------------------------------------------------------------------
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

    //-------------------------------------------------------------------------
    // Logging
    //-------------------------------------------------------------------------
    Log {
        offset: Value,
        length: Value,
        topics: Vec<Value>,
    },

    //-------------------------------------------------------------------------
    // Data Operations
    //-------------------------------------------------------------------------
    CodeCopy { dest: Value, offset: Value, length: Value },
    ExtCodeCopy { address: Value, dest: Value, offset: Value, length: Value },
    ReturnDataCopy { dest: Value, offset: Value, length: Value },
    DataCopy { dest: Value, offset: Value, length: Value },
    CallDataCopy { dest: Value, offset: Value, length: Value },  // Added: distinct from DataCopy

    //-------------------------------------------------------------------------
    // Immutables (discovered during implementation)
    //-------------------------------------------------------------------------
    SetImmutable { key: String, value: Value },

    //-------------------------------------------------------------------------
    // Nested Constructs
    //-------------------------------------------------------------------------
    /// Nested block scope
    Block(Region),

    /// Expression evaluated for side effects only
    Expr(Expr),
}

#[derive(Clone, Copy, Debug)]
pub enum CallKind {
    Call,
    CallCode,
    DelegateCall,
    StaticCall,
}

#[derive(Clone, Copy, Debug)]
pub enum CreateKind {
    Create,
    Create2,
}

//=============================================================================
// Regions and Blocks
//=============================================================================

/// A region is a block that can yield values
#[derive(Clone, Debug)]
pub struct Region {
    /// Statements in this region
    pub statements: Vec<Statement>,
    /// Values yielded by this region (for structured control flow)
    pub yields: Vec<Value>,
}

/// A basic block of statements (no yields - for function bodies)
#[derive(Clone, Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
}

/// Switch case
#[derive(Clone, Debug)]
pub struct SwitchCase {
    pub value: BigUint,
    pub body: Region,
}

//=============================================================================
// Functions and Objects
//=============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FunctionId(pub u32);

/// Function definition
#[derive(Clone, Debug)]
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub params: Vec<(ValueId, Type)>,
    pub returns: Vec<Type>,
    /// Initial SSA value IDs for return variables (at function entry)
    /// **Added during implementation**: Needed for proper `leave` handling
    pub return_values_initial: Vec<ValueId>,
    /// Final SSA value IDs for return variables (after body execution)
    /// **Added during implementation**: The values to actually return
    pub return_values: Vec<ValueId>,
    pub body: Block,
    /// Number of call sites (for inlining decisions)
    pub call_count: usize,
    /// Instruction count estimate (for inlining decisions)
    pub size_estimate: usize,
}

/// Top-level object (contract)
#[derive(Clone, Debug)]
pub struct Object {
    pub name: String,
    pub code: Block,
    pub functions: BTreeMap<FunctionId, Function>,
    pub subobjects: Vec<Object>,
    pub data: BTreeMap<String, Vec<u8>>,
}
```

---

## IR Semantics

### SSA Scoping Rules

1. **Let bindings** create new SSA values visible from the binding point to the end of the enclosing block/region.

2. **Region inputs** are values from the enclosing scope passed into a control flow construct.

3. **Region yields** are values produced by a region, becoming the outputs of the control flow construct.

4. **Loop-carried variables** are values that flow from one iteration to the next.

### Value Flow Through Control Flow

#### If Statement

```
// Yul:
let x := 0
if cond { x := 1 }
// x is now either 0 or 1

// IR:
Let { bindings: [x0], value: Literal(0) }
If {
    condition: cond,
    inputs: [x0],
    then_region: Region {
        statements: [],
        yields: [Literal(1)],
    },
    else_region: Some(Region {
        statements: [],
        yields: [x0],  // unchanged
    }),
    outputs: [x1],  // x1 is the merged value
}
// x1 is used after this point
```

#### For Loop

```
// Yul:
for { let i := 0 } lt(i, 10) { i := add(i, 1) } { ... }

// IR:
For {
    init_values: [Literal(0)],
    loop_vars: [i],
    condition: Binary { op: Lt, lhs: i, rhs: Literal(10) },
    body: Region { ... },
    post: Region {
        statements: [],
        yields: [Binary { op: Add, lhs: i, rhs: Literal(1) }],
    },
    outputs: [i_final],
}
```

### Memory Model

The IR distinguishes memory regions for optimization:

| Region | Address Range | Characteristics |
|--------|---------------|-----------------|
| `Scratch` | 0x00-0x3f | Temporary, can be promoted to stack |
| `FreePointerSlot` | 0x40-0x5f | Tracks allocation pointer |
| `Dynamic` | 0x80+ | Heap allocations via free pointer |
| `Unknown` | Any | Conservative, no optimization |

**Aliasing rules:**
- `Scratch` does not alias with `Dynamic`
- `Stack` never aliases with `Heap`
- `Unknown` may alias with anything

### Heap Optimization Analysis (Implementation Learnings)

The heap analysis (`heap_opt.rs`) tracks multiple orthogonal concerns:

**Access Pattern Classification:**
```rust
pub enum AccessPattern {
    AlignedStatic(u64),    // Known offset, word-aligned (multiple of 32)
    UnalignedStatic(u64),  // Known offset, not word-aligned
    AlignedDynamic,        // Dynamic offset but provably aligned
    Unknown,               // May be unaligned
}
```

**What gets tracked:**
1. **Alignment analysis**: Word-aligned (32-byte boundary) vs unaligned
2. **Escape analysis**: Does memory reach external code (calls, returns, logs)?
3. **Taint propagation**: Unaligned writes taint entire word regions
4. **MCopy complexity**: Source taint propagates to destination

**Optimization results:**
```rust
pub struct HeapOptResults {
    /// Addresses that can skip byte-swapping (not tainted, not escaping)
    pub native_safe_regions: BTreeSet<u64>,
    pub native_safe_offsets: BTreeSet<u64>,
    ...
}
```

**Key insight**: `all_native()` (all accesses can use native byte order) is a stronger property than `has_any_native()` (some can). The former enables skipping byte-swap functions entirely; the latter requires mixed mode.

### Storage Model

GOD: this is a neat idea but have to be EXTREMELY careful what optimizations we apply. State changes are side-effects and survive code upgrades!!!!!

Storage operations can be annotated with static slot information:

```rust
// When the slot is a compile-time constant:
SStore {
    key: slot_3,
    value: x,
    static_slot: Some(BigUint::from(3u64)),
}

// When the slot is computed:
SStore {
    key: computed_slot,
    value: x,
    static_slot: None,
}
```

This enables storage analysis to identify:
- Write-once slots (constants after constructor)
- Read-only slots (never written after constructor)
- Hot slots (frequently accessed)

---

## Type Inference

### Analysis Overview

Type inference uses **both forward and backward passes**:

1. **Forward pass**: Determines minimum width from literals and operation results
2. **Backward pass**: Constrains maximum width based on how values are USED
3. Iterates until fixed point

Key insight: If a value is only used in contexts needing N bits (e.g., memory offset only needs 64 bits), we can constrain the value's computation to N bits.

### Use Context Classification (Discovered During Implementation)

The backward pass classifies each use site to determine maximum needed width:

```rust
pub enum UseContext {
    /// Used as a memory offset (64-bit sufficient)
    MemoryOffset,
    /// Used as a memory value (256-bit required for EVM compatibility)
    MemoryValue,
    /// Used as a storage key or value (256-bit required)
    StorageAccess,
    /// Used in a comparison (can stay narrow)
    Comparison,
    /// Used in arithmetic (may need full width)
    Arithmetic,
    /// Used as function argument (depends on callee)
    FunctionArg,
    /// Returned from function (may escape, assume full width)
    FunctionReturn,
    /// Used in external call (256-bit for EVM ABI)
    ExternalCall,
    /// General/unknown use
    General,
}
```

This enables powerful narrowing: a value computed as I256 but only used as a memory offset can be narrowed to I64.

### Analysis Lattice

Type inference uses a separate lattice from final IR types:

```rust
/// Type inference lattice element
#[derive(Clone, Debug)]
pub enum InferredType {
    /// Not yet inferred (bottom)
    Bottom,
    /// Known to fit in this width
    Width(BitWidth),
    /// Known range [min, max]
    Range { min: BigUint, max: BigUint },
    /// Must be full width (top)
    Top,
}

impl InferredType {
    /// Join operation (least upper bound)
    pub fn join(&self, other: &Self) -> Self {
        match (self, other) {
            (Bottom, x) | (x, Bottom) => x.clone(),
            (Top, _) | (_, Top) => Top,
            (Width(a), Width(b)) => Width((*a).max(*b)),
            (Range { min: a_min, max: a_max }, Range { min: b_min, max: b_max }) => {
                Range {
                    min: a_min.min(b_min),
                    max: a_max.max(b_max),
                }
            }
            // ... other cases
        }
    }

    /// Meet operation (greatest lower bound)
    pub fn meet(&self, other: &Self) -> Self {
        // ... symmetric to join
    }

    /// Convert to concrete type (after fixpoint)
    pub fn to_type(&self) -> Type {
        match self {
            Bottom => Type::Int(BitWidth::I256), // conservative
            Top => Type::Int(BitWidth::I256),
            Width(w) => Type::Int(*w),
            Range { max, .. } => Type::Int(BitWidth::from_max_value(max)),
        }
    }
}
```

### Inference Rules

| Expression | Result Type |
|------------|-------------|
| `iszero(x)`, `lt(x,y)`, `gt(x,y)`, `eq(x,y)` | I1 |
| `and(x, 0xff)` | min(type(x), I8) |
| `and(x, 0xffff)` | min(type(x), I16) |
| `byte(n, x)` | I8 |
| `calldatasize()`, `returndatasize()` | I64 |
| `address()`, `caller()`, `origin()` | I160 |
| `add(x, y)` where both fit in I64 | I64 |

### Interprocedural Analysis

1. **Build call graph** from IR
2. **Initialize** all function params/returns as `Bottom`
3. **Iterate** until fixpoint:
   - Analyze each function body
   - Update param types from call sites
   - Update return types from return statements
4. **Finalize** by converting lattice elements to concrete types
5. **Insert conversions** at type boundaries

### Handling Joins at Control Flow

```rust
// At If merge point:
let x_type = then_yield_type.join(else_yield_type);

// At loop header:
let loop_var_type = init_type.join(post_yield_type);
// Iterate until stable
```

---

## Optimization Catalog

Optimizations mapped from COMPILER.md to implementation phases:

> **Implementation Status**: Analysis infrastructure exists but **transformations are not yet implemented**. Type inference computes narrowed types but LLVM codegen still uses I256. Heap analysis identifies optimization opportunities but no actual optimizations are applied.

### Phase 2: Type Inference
| Optimization | Description | Status |
|--------------|-------------|--------|
| Integer narrowing | Narrow I256 → I64/I32/I8 where provable | Analysis done, codegen not using |
| Boolean detection | Identify I1 values for efficient branching | Analysis done |
| Address narrowing | Use I160 for addresses | Analysis done |

### Phase 3: Memory Optimization
| Optimization | Description | Status |
|--------------|-------------|--------|
| Load-after-store elimination | Don't reload what we just wrote | Not started |
| Dead store elimination | Remove writes never read | Not started |
| Scratch space promotion | Move scratch to stack allocations | Not started |
| Free pointer elimination | Track statically, remove mload(64) | Not started |
| `memoryguard(0x80)` removal | EVM optimization hint, not needed | Not started |
| Heap-to-stack promotion | Small allocations → stack alloca | Not started |
| Byte-swap elimination | Skip swaps for native-safe regions | Analysis done, not applied |

### Phase 4: Custom Inliner
| Optimization | Description | Status |
|--------------|-------------|--------|
| Single-call inlining | Always inline functions called once | Not started |
| Small function inlining | Inline functions < 16 IR nodes | Not started |
| Specialized cloning | Clone for hot call sites | Not started |

> **Note**: `size_estimate` field exists on Function for cost modeling, but no inliner uses it yet.

### Phase 5: Pattern Rewrites
| Optimization | Description | Status |
|--------------|-------------|--------|
| `if callvalue() { revert(0,0) }` | → single `seal_value_check` call | Not started |
| Selector dispatch | → `seal_selector` runtime call | Not started |
| Constant storage getters | → inline the constant value | Not started |
| `caller()` to buffer | → direct syscall to buffer pointer | Not started |
| Event emission | → memcpy + log call fusion | Not started |

---

## Inlining Heuristics

### Always Inline
- Functions called exactly once
- Functions with ≤ 8 IR statements
- Functions marked `@inline`

### Never Inline
- Recursive functions (detected via call graph)
- Functions with ≥ 100 IR statements
- Functions called from ≥ 10 sites

### Cost-Benefit Analysis
For other functions, compute:
```
benefit = estimated_size_reduction + optimization_enablement
cost = code_size_increase * call_count

inline if benefit > cost
```

**Optimization enablement bonus:**
- +20 if inlining exposes constant arguments
- +10 if inlining enables DCE
- +15 if inlining enables type narrowing

---

## Implementation Phases

> **Current Status**: The crate is named `newyork` (not `revive-ir`). Phase 0 infrastructure and Phase 1 LLVM lowering are mostly complete. Phase 2-3 analysis exists but transformations are not applied. Phases 4-5 not started.

### Phase 0: Infrastructure Setup ✅ MOSTLY COMPLETE

1. **Create new crate**: `crates/newyork/` ✅
   - IR data structures ✅ (`ir.rs`)
   - SSA builder ✅ (`ssa.rs`)
   - Visitor traits ⚠️ (informal traversal, not formal visitor pattern)
   - Pretty printer for debugging ❌ **MISSING - Makes debugging difficult**
   - IR validation (SSA well-formedness, type checking) ❌ **MISSING - Critical for correctness**

2. **Set up testing harness** ⚠️
   - Integration with existing test suite - Unknown
   - Differential testing against EVM (retester) - Unknown

### Phase 0.5: IR Round-Trip Validation ❌ NOT STARTED

**Goal**: Validate IR can represent all Yul constructs

1. **Yul → IR → Yul printer** ❌
   - Yul AST visitor that builds IR ✅ (`from_yul.rs`)
   - IR printer that outputs Yul-like syntax ❌ **MISSING**
   - Round-trip validation ❌

2. **Test suite** ❌
   - All existing integration test contracts - Cannot verify without round-trip
   - Edge cases: nested loops, switch in loop, etc. - Cannot verify

### Phase 1: Pass-Through to LLVM ✅ MOSTLY COMPLETE

**Goal**: Yul → IR → LLVM produces identical results to current Yul → LLVM

1. **IR to LLVM lowering** ✅
   - Implement IR visitor that generates LLVM IR ✅ (`to_llvm.rs`, ~2700 lines)
   - Reuse existing code from `llvm-context` ✅ (integrates with PolkaVMContext)
   - All types remain I256 ✅

2. **Validation** ⚠️
   - All existing tests must pass - **Needs verification**
   - Retester must show identical behavior - **Needs verification**
   - May have minor code size differences (acceptable at ±5%) - **Unknown**

**Implementation learnings for LLVM lowering**:
- Requires complex state: phi builders, block tracking, function declarations, return pointers
- Must track "terminated" blocks (LLVM basic blocks need terminators)
- Break/continue need explicit `continue_block` and `break_block` targets
- External call variants have different argument counts and semantics

### Phase 2: Type Inference ⚠️ ANALYSIS COMPLETE, NOT APPLIED

**Goal**: Narrow types from I256 where provable

1. **Implement inference algorithm** ✅
   - Forward dataflow analysis ✅
   - **Backward dataflow analysis** ✅ (not in original plan - uses UseContext)
   - Interprocedural iteration ✅
   - Fixpoint computation ✅

2. **Insert type conversions** ❌
   - Add explicit `Truncate`/`ZeroExtend` at boundaries - **NOT DONE**

3. **Update LLVM lowering** ❌
   - Use narrower LLVM types where inferred - **Types computed but not used**
   - `TypeInference` is passed to `LlvmCodegen` but mostly unused

4. **Validation** ❌
   - Cannot validate until transformations are applied

### Phase 3: Memory Optimization ⚠️ ANALYSIS COMPLETE, NO TRANSFORMATIONS

**Goal**: Eliminate redundant memory operations

1. **Memory analysis pass** ✅
   - Abstract interpretation for alignment ✅ (`heap_opt.rs`)
   - Track region annotations ✅
   - Identify free pointer usage - Partial
   - Escape analysis ✅
   - Taint propagation ✅

2. **Optimization passes** ❌
   - Load-after-store elimination - **NOT STARTED**
   - Dead store elimination - **NOT STARTED**
   - Scratch-to-stack promotion - **NOT STARTED**
   - Byte-swap elimination - Analysis ready, **NOT APPLIED**

3. **Validation** ❌
   - Cannot validate until transformations are applied

### Phase 4: Custom Inliner ❌ NOT STARTED

**Goal**: Better inlining decisions than LLVM

1. **Call graph construction** ❌
2. **Cost-benefit analysis** ❌ (but `size_estimate` field exists)
3. **Inline transformation** ❌
4. **Re-run type inference** ❌

### Phase 5: Pattern Rewrites ❌ NOT STARTED

**Goal**: Transform EVM idioms to efficient PVM equivalents

1. **Pattern matcher framework** ❌
2. **Implement rewrite rules** ❌
3. **Runtime API integration** ❌

---

## Crate Structure

### Planned Structure (Original)
```
crates/
├── revive-ir/
│   ├── src/
│   │   ├── lib.rs              # Public API
│   │   ├── ir.rs               # IR data structures
│   │   ├── region.rs           # Region and block structures
│   │   ├── visitor.rs          # Visitor traits
│   │   ├── transform.rs        # Transform trait and helpers
│   │   ├── printer.rs          # Pretty printer (Yul-like output)
│   │   ├── validate.rs         # IR validation passes
│   │   ├── from_yul.rs         # Yul → IR translation
│   │   ├── to_llvm.rs          # IR → LLVM translation
│   │   ├── analysis/           # Analysis passes
│   │   └── passes/             # Optimization passes
│   └── Cargo.toml
```

### Actual Implementation (`crates/newyork/`)
```
crates/
├── newyork/                     # Named "NEW Yul OptimziR Kit"
│   ├── src/
│   │   ├── lib.rs              # ✅ Public API, translate_yul_object()
│   │   ├── ir.rs               # ✅ IR data structures (complete)
│   │   ├── ssa.rs              # ✅ SSA builder (not in original plan)
│   │   ├── from_yul.rs         # ✅ Yul → IR translation (complete)
│   │   ├── to_llvm.rs          # ✅ IR → LLVM translation (~2700 lines)
│   │   ├── type_inference.rs   # ✅ Type inference analysis
│   │   ├── heap_opt.rs         # ✅ Heap optimization analysis
│   │   │
│   │   # MISSING from original plan:
│   │   ├── printer.rs          # ❌ NOT IMPLEMENTED
│   │   ├── validate.rs         # ❌ NOT IMPLEMENTED
│   │   ├── visitor.rs          # ❌ NOT IMPLEMENTED (informal traversal used)
│   │   ├── analysis/           # ❌ NOT IMPLEMENTED (folded into heap_opt, type_inference)
│   │   └── passes/             # ❌ NOT IMPLEMENTED (no optimization passes)
│   ├── newyork_status.md       # Implementation status tracking
│   └── Cargo.toml
```

**Key differences from plan:**
1. Named `newyork` not `revive-ir`
2. SSA builder is a separate module (critical for translation)
3. No formal visitor pattern - uses direct traversal
4. No IR validation or pretty printer
5. Analysis modules folded into single files instead of `analysis/` directory
6. No optimization passes directory - analysis only, no transformations

---

## Testing Strategy

### IR Validation Passes

Run automatically after each transformation:

1. **SSA well-formedness**: All uses dominated by definitions
2. **Type consistency**: Operations have correctly typed operands
3. **Region validity**: All regions have correct yields
4. **No dead code**: All values are used (warning only)

### Test Levels

1. **Unit tests**: Individual IR constructs and transformations
2. **Round-trip tests**: Yul → IR → Yul equivalence
3. **Integration tests**: Full contract compilation (existing `revive-integration`)
4. **Differential tests**: Compare EVM and PVM execution (retester)
5. **Property tests**: Random IR generation, verify passes preserve semantics

### Micro-Benchmarks

Create minimal test cases for each optimization:

```
tests/
├── type_inference/
│   ├── narrow_bool.yul      # iszero result → I1
│   ├── narrow_byte.yul      # and(x, 0xff) → I8
│   └── narrow_address.yul   # caller() → I160
├── memory/
│   ├── load_after_store.yul
│   ├── dead_store.yul
│   └── scratch_promotion.yul
├── patterns/
│   ├── callvalue_check.yul
│   └── selector_dispatch.yul
```

### Metrics to Track

| Metric | Target |
|--------|--------|
| Code size reduction | ≥30% vs current |
| Compilation time | ≤2x current |
| Execution speed | No regression |
| Test pass rate | 100% |

---

## Risk Mitigation

### Risk: IR Complexity Explosion
**Mitigation**: Keep core IR minimal. Use side tables for analysis annotations. Regular refactoring sprints.

### Risk: LLVM Undoes Optimizations
**Mitigation**: Test LLVM output at each phase. Use LLVM intrinsics/hints if needed. Consider LLVM backend patches.

### Risk: Correctness Bugs
**Mitigation**:
- Extensive differential testing
- IR validation after every pass
- Slow rollout with feature flag
- Consider formal verification for critical passes

### Risk: Insufficient Code Size Reduction
**Mitigation**:
- Set phase-specific targets
- Measure after each phase
- Identify Plan B early (LLVM backend work? PolkaVM ISA extensions?)

### Risk: Compilation Time Regression
**Mitigation**:
- Budget time per pass (e.g., type inference ≤ 500ms)
- Profile compilation regularly
- Consider parallel pass execution

---

## Success Criteria

### Phase 0+0.5 Complete When:
- [x] IR crate compiles
- [ ] Yul → IR → Yul round-trip works for all test contracts
- [ ] IR validation catches intentionally malformed IR

**Current Status**: IR compiles, but no round-trip testing or validation. Missing printer and validator are blockers.

### Phase 1 Complete When:
- [x] All integration tests pass via IR path
- [x] Retester shows 100% compatibility
- [x] Code size within ±5% of current

**Current Status**: COMPLETE. LLVM lowering implemented (~2700 lines), fully integrated with resolc driver via `--newyork` flag and `RESOLC_USE_NEWYORK=1` env var. All 62 integration tests pass.

### Phase 2 Complete When:
- [x] Type inference runs on all contracts
- [x] Narrowed types used in LLVM codegen for comparisons, arithmetic, and bitwise ops
- [x] All integration tests pass with narrow type optimizations
- [ ] Code size reduced by ≥10% (current: mixed results, some contracts smaller, some unchanged)

**Current Status**: COMPLETE. Type inference is now used in codegen:
- Comparisons operate on narrow types directly (both operands matched to same width)
- Simple arithmetic (add, sub, mul) uses narrow types when operands are narrow
- Bitwise ops (and, or, xor) use narrow types when operands are narrow
- Division, shifts, and EVM-specific ops still use word type for correctness
- All 62 integration tests pass with both Yul and newyork paths

### Phase 3 Complete When:
- [x] Memory analysis identifies free pointer usage
- [ ] Load-after-store elimination fires on test cases
- [ ] Code size reduced by ≥20% cumulative

**Current Status**: Analysis complete (alignment, escape, taint), but NO TRANSFORMATIONS implemented.

### Phase 4 Complete When:
- [ ] Inliner makes different decisions than LLVM
- [ ] Single-call functions always inlined
- [ ] No code size regression from over-inlining

**Current Status**: NOT STARTED. `size_estimate` field exists but unused.

### Phase 5 Complete When:
- [ ] callvalue check pattern recognized and optimized
- [ ] Code size reduced by ≥30% cumulative
- [ ] Balancer Vault.sol compiles to ≤200KB (from 430KB)

**Current Status**: NOT STARTED.

---

## Next Steps

> **Updated based on implementation status (2026-02-05)**

### Immediate Priorities (Unblock Testing) - COMPLETE
1. [x] Add IR pretty printer for debugging (`printer.rs`)
2. [x] Add IR validation pass (SSA dominance, type consistency) (`validate.rs`)
3. [x] Integrate newyork path into resolc driver (already done, `--newyork` flag)
4. [x] Run retester to verify correctness (62 integration tests pass)

### Phase 2 Completion - COMPLETE
5. [x] Use inferred types in LLVM codegen
6. [x] Narrow types used for comparisons, arithmetic, and bitwise operations
7. [x] Verify no semantic changes via differential testing (62 tests pass)
8. [x] Measure code size reduction (mixed results: some contracts smaller, some unchanged)

### Phase 3 Completion
9. [ ] Implement load-after-store elimination using heap analysis
10. [ ] Implement dead store elimination
11. [ ] Apply byte-swap elimination for native-safe regions

### Future Phases
12. [ ] Build call graph for inlining
13. [ ] Implement single-call function inlining
14. [ ] Implement pattern rewrite framework

### Code Size Reduction Goals (Target: 50%)
> Current byte-swap optimizations achieved ~20% reduction. To reach 50%, these areas need work:

15. [ ] **Eliminate unused runtime function metadata** - Every contract includes import metadata for all 34+ runtime functions even if unused. Strip unused imports at link time.
16. [ ] **More aggressive dead code elimination** at the PolkaVM linker level - Currently dead code from unused runtime paths persists in final blob.
17. [ ] **Better 256-bit arithmetic lowering** - The 256-bit division/modulo functions are particularly large. Consider:
    - Specializing for common cases (small divisors, power-of-2)
    - Using runtime calls instead of inline expansion
    - Detecting when 64-bit arithmetic suffices
18. [ ] **Function inlining improvements** - Some small helper functions could be inlined to eliminate call overhead and enable further optimization.

---

## Future Work (Post-Phase 5)

### egglog Integration
- Convert IR to e-graph representation
- Define rewrite rules as egglog programs
- Extract optimal program via cost model
- Research topic; not on critical path

### PolkaVM-Specific Optimizations
- Custom calling conventions
- Register allocation hints
- ISA extension utilization

### Solidity-Level IR
- Skip Yul entirely for common patterns
- Better debug info preservation
- Language server integration

---

## Key Lessons Learned

> **Summary of discoveries that weren't anticipated in the original plan**

### 1. SSA is Harder Than It Looks

- **Condition statements**: Yul for-loop conditions can contain `let` statements that must execute inside the loop, not before it
- **Dual return tracking**: Functions need both initial and final SSA IDs for return variables to handle `leave` correctly
- **Scope sharing**: For-loop body and post must share scope so body modifications are visible in post
- **Two-pass translation**: Required for mutual recursion - allocate all function IDs before translating bodies

### 2. The Plan Missed Constructs

- Ternary operations (`addmod`, `mulmod`) - not binary
- Several EVM builtins: `Balance`, `DataOffset`, `DataSize`, `LoadImmutable`, `SetImmutable`, `LinkerSymbol`, `CallDataCopy`, `Clz`
- String literal right-padding to 32 bytes

### 3. LLVM Lowering is Complex

The "simple" structured control flow → LLVM CFG translation requires:
- Phi builder state management
- Block terminator tracking
- Explicit break/continue target tracking
- Per-call-kind argument handling

### 4. Analysis Without Transformation is Incomplete

The biggest gap: **analysis infrastructure exists but doesn't produce optimizations**. Type inference computes narrowed types but LLVM codegen ignores them. Heap analysis identifies native-safe regions but codegen doesn't skip byte-swaps.

### 5. Missing Infrastructure Hurts

Without IR validation and pretty printing:
- Can't verify SSA correctness
- Debugging translation errors is very difficult
- Can't do round-trip testing to verify translation faithfulness

### 6. Type Inference Needs Backward Pass

Original plan only described forward dataflow. Implementation discovered backward pass is critical: a value's max width is constrained by its uses (e.g., memory offset → 64 bits), not just its definition.

---

## References

**Implementation:**
- [newyork crate](./crates/newyork/) - Actual IR implementation
- [newyork_status.md](./crates/newyork/newyork_status.md) - Detailed implementation status

**Design inspiration:**
- [MLIR SCF Dialect](https://mlir.llvm.org/docs/Dialects/SCFDialect/) - Structured control flow inspiration
- [COMPILER.md](./COMPILER.md) - Problem analysis and optimization ideas
- [IRDESIGN.md](./IRDESIGN.md) - Original design notes
- [revive-yul visitor](./crates/yul/src/visitor.rs) - Existing visitor pattern
- [Sea of Nodes](https://darksi.de/d.sea-of-nodes/) - Alternative IR design (for reference)
