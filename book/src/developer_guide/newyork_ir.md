# newyork IR reference

A per-operation reference for the newyork IR: textual syntax, operand and result types, purity, region and static-slot annotations, and examples.

## How to read this reference

This reference page enumerates every operation the newyork IR supports. It is a lookup, not a walkthrough: each entry is self-contained and intended to be reachable by anchor.

Operations are grouped by function (memory and storage writes, pure expressions, control flow, and so on) rather than alphabetically. Jump to a specific operation from the [operation index](#operation-index) below, or use the sidebar.

Every operation appears in two places in the codebase. The canonical Rust definition is a variant of either `Expression` or `Statement` in `ir.rs`. The textual rendering used by debug dumps and by this reference page is produced by the printer in `printer.rs`. Treat the printed syntax as a debug surface, not a stable input language: there is no parser for it, and printer details change when passes add new annotations.

### Entry format

Each operation entry has the same shape:

| Field | What it shows |
|---|---|
| **Heading** | The printed operation name (e.g. `mstore`) followed by the `Expression` or `Statement` variant it corresponds to in `ir.rs`. |
| **Description** | A short prose summary of what the operation does and any semantic notes worth knowing before reading the rest of the entry. |
| **Syntax** | The literal printer output, including any optional debug annotations (region tags, static-slot comments). Anything inside `/* ... */` is a debug-only annotation and is not part of the operation itself. |
| **Example** | A minimal printed snippet, using the printer's actual `v0`/`v1`/… naming. |
| **Operands** | One row per input or structural participant in the printed syntax. Value operands list the narrowest type the operation guarantees (default `i256`; narrower widths only appear when type inference has narrowed an upstream definition). Vector-of-operands fields show `Vec<…>` as the type. Non-value participants such as nested regions are listed with an em-dash type to mark them as structural rather than as operands. |
| **Result and purity** | The type the operation produces (or *none* for statements that bind no value), followed by a purity label, either *Pure* or *Effectful*. Pure operations may be reordered, deduplicated, or eliminated by the simplifier; effectful ones may not. Effectful entries may carry a parenthetical describing the nature of the side effect when informative (e.g. "control flow", "terminator", or a note about revert/trap behavior). |
| **Annotations** | Operation-specific fields the printer surfaces as `/* ... */` comments in the dump (region tag for memory ops, static-slot hint for storage ops, type suffix for non-default widths). Listed here as a table of *source field* → *printed form*. |

### Syntax notation

Syntax templates in each entry use the following conventions:

| Notation | Meaning |
|---|---|
| `add`, `mload`, `if`, `else`, `case`, `let`, `yield`, … | Literal printer tokens: bare lowercase identifiers and keywords that the printer emits verbatim. |
| `$offset`, `$value`, `$key`, `$lhs`, `$rhs`, … | Role names (`$`-prefixed): placeholders for SSA value references the printer renders as `v` followed by a decimal id (`v0`, `v1`, …). |
| `<type>`, `<region>`, `<hex>`, `<id>`, `<bits>`, `<func_name>`, `<N>`, `<length>`, … | Metavariables: stand for compile-time fields (type tags, hex values, identifier strings, integer counts), not SSA values. The concrete values they take are enumerated in the Annotations section of each entry or in the type system reference. |
| `[…]` | Optional parts. Anything inside the brackets may or may not appear in any given dump, depending on the conditions described in the operation's Annotations section. |
| `[: <type>]` | Optional type suffix on a value reference. Suppressed when the value's type is the default `i256` integer; present otherwise (`: i32`, `: ptr<heap>`, …). |
| `/* … */` | Debug-only annotations the printer attaches to certain operations (memory region tag, static-slot hint, etc.). |
| `…` | Repetition: "more entries of the same shape." Used in vector operand lists (`$arg_0, $arg_1, …`) and in multi-line block bodies (`{ … }`). |

### Operation index

#### Pure expressions

##### Constants and variables

- [`0x<hex>`](#0xhex)
- [`v<id>`](#vid)

##### Arithmetic

- [`add`](#add)
- [`sub`](#sub)
- [`mul`](#mul)
- [`div`](#div)
- [`sdiv`](#sdiv)
- [`mod`](#mod)
- [`smod`](#smod)
- [`exp`](#exp)
- [`and`](#and)
- [`or`](#or)
- [`xor`](#xor)
- [`shl`](#shl)
- [`shr`](#shr)
- [`sar`](#sar)
- [`lt`](#lt)
- [`gt`](#gt)
- [`slt`](#slt)
- [`sgt`](#sgt)
- [`eq`](#eq)
- [`byte`](#byte)
- [`signextend`](#signextend)
- [`addmod`](#addmod)
- [`mulmod`](#mulmod)
- [`iszero`](#iszero)
- [`not`](#not)
- [`clz`](#clz)

##### Bit-width conversions

- [`truncate<i<bits>>`](#truncateibits)
- [`zext<i<bits>>`](#zextibits)
- [`sext<i<bits>>`](#sextibits)

##### Hashing

- [`keccak256`](#keccak256)
- [`keccak256_pair`](#keccak256_pair)
- [`keccak256_single`](#keccak256_single)

##### Environment reads

- [`caller`](#caller)
- [`callvalue`](#callvalue)
- [`origin`](#origin)
- [`address`](#address)
- [`chainid`](#chainid)
- [`gas`](#gas)
- [`msize`](#msize)
- [`coinbase`](#coinbase)
- [`timestamp`](#timestamp)
- [`number`](#number)
- [`difficulty`](#difficulty)
- [`gaslimit`](#gaslimit)
- [`basefee`](#basefee)
- [`blobbasefee`](#blobbasefee)
- [`blobhash`](#blobhash)
- [`blockhash`](#blockhash)
- [`selfbalance`](#selfbalance)
- [`gasprice`](#gasprice)

##### Calldata, returndata, and code

- [`calldataload`](#calldataload)
- [`calldatasize`](#calldatasize)
- [`returndatasize`](#returndatasize)
- [`codesize`](#codesize)
- [`extcodesize`](#extcodesize)
- [`extcodehash`](#extcodehash)
- [`balance`](#balance)

##### Memory and storage loads

- [`mload`](#mload)
- [`sload`](#sload)
- [`tload`](#tload)
- [`mapping_sload`](#mapping_sload)

##### Linker

- [`dataoffset`](#dataoffset)
- [`datasize`](#datasize)
- [`loadimmutable`](#loadimmutable)
- [`linkersymbol`](#linkersymbol)

##### Function call

- [`<func_name>`](#func_name)

#### Memory and storage writes

- [`mstore`](#mstore)
- [`mstore8`](#mstore8)
- [`mcopy`](#mcopy)
- [`sstore`](#sstore)
- [`tstore`](#tstore)
- [`mapping_sstore`](#mapping_sstore)

#### Bulk copies

- [`codecopy`](#codecopy)
- [`extcodecopy`](#extcodecopy)
- [`returndatacopy`](#returndatacopy)
- [`datacopy`](#datacopy)
- [`calldatacopy`](#calldatacopy)

#### Bindings and wrappers

- [`let`](#let)
- [expression statement](#expression-statement)
- [`setimmutable`](#setimmutable)

#### Structured control flow

- [`if`](#if)
- [`switch`](#switch)
- [`for`](#for)
- [`break`](#break)
- [`continue`](#continue)
- [`leave`](#leave)
- [nested block](#nested-block)

#### External interaction

- [`call`](#call)
- [`callcode`](#callcode)
- [`delegatecall`](#delegatecall)
- [`staticcall`](#staticcall)
- [`create`](#create)
- [`create2`](#create2)
- [`log<N>`](#logn)

#### Termination

- [`return`](#return)
- [`revert`](#revert)
- [`stop`](#stop)
- [`invalid`](#invalid)
- [`selfdestruct`](#selfdestruct)
- [`panic_revert`](#panic_revert)
- [`error_string_revert`](#error_string_revert)
- [`custom_error_revert`](#custom_error_revert)

## Type system

Every value in the IR carries a `Type`. The operation entries below refer to widths (`i1`…`i256`), address spaces (`ptr<heap>`, etc.), and memory regions (`scratch`, etc.) by their printed form; this section is the reference for those names.

### `Type`

The umbrella enum, with these variants:

| Variant | Printed as | Description |
|---|---|---|
| `Int(BitWidth)` | `i1`, `i8`, …, `i256` | An integer at one of the [BitWidth](#bitwidth) widths. |
| `Ptr(AddressSpace)` | `ptr<heap>`, `ptr<stack>`, `ptr<storage>`, `ptr<code>` | A pointer tagged with its address space; see [AddressSpace](#addressspace). |
| `Void` | `void` | Unit type. Used for statements that produce no value and for `void`-returning functions. |

### `BitWidth`

The rungs of integer width. Newly minted values default to `I256`; type inference narrows them down to one of the lower rungs when it can prove the upper bits are zero or unused.

| Variant | Printed as | Typical use |
|---|---|---|
| `I1` | `i1` | Boolean. Result type of every comparison and `iszero`. |
| `I8` | `i8` | Byte values. The narrowest meaningful integer. |
| `I32` | `i32` | PolkaVM pointer width (XLEN); minimum width for function parameters under the rv64e ABI. |
| `I64` | `i64` | PolkaVM native register width; most narrowed values land here. |
| `I128` | `i128` | Two registers; arithmetic that overflows `i64` but doesn't need full 256-bit emulation. |
| `I160` | `i160` | Ethereum addresses; result of `caller`, `origin`, mapping keys. |
| `I256` | `i256` | EVM word width. The default and conservative ceiling. |

### `AddressSpace`

The address space a pointer points into. Carried on every `Ptr` value so the codegen can lower loads and stores without a separate alias-analysis pass.

| Variant | Printed as | Points into | Endianness |
|---|---|---|---|
| `Heap` | `ptr<heap>` | Emulated EVM linear memory (the simulated `mload`/`mstore` region). | Big-endian (by EVM contract). |
| `Stack` | `ptr<stack>` | Native PolkaVM stack allocations. | Little-endian (no swap). |
| `Storage` | `ptr<storage>` | Contract storage; key/value with 256-bit slots. | Big-endian on the wire. |
| `Code` | `ptr<code>` | Read-only code/data segment. | Big-endian. |

### `MemoryRegion`

A refinement carried by every memory load and store on top of `AddressSpace::Heap`. The tag tells later passes what kind of heap address an offset is hitting, which drives both free-memory-pointer propagation and byte-swap elimination.

| Variant | Address range | Printed as | Meaning |
|---|---|---|---|
| `Scratch` | `0x00`–`0x3f` | `/* scratch */` | EVM scratch space; safe to touch without consulting the free memory pointer. |
| `FreePointerSlot` | exactly `0x40` | `/* free_ptr */` | Slot that stores the free memory pointer itself. |
| `Dynamic` | `0x80` and above | `/* dynamic */` | Real heap allocations. |
| `Unknown` | everything else (constants in `0x41`–`0x7f`, plus all non-constant offsets) | (suppressed) | Conservative fallback used when the offset isn't a constant or doesn't slot cleanly. |

## Pure expressions

Pure expressions produce values without side effects. The simplifier may freely reorder, deduplicate, and eliminate them. They appear on the right-hand side of a `let` binding, or as operands of other expressions and effectful statements; the operand positions accept SSA value references only, so any pure expression that is consumed elsewhere is first bound by a `let`. Examples in this section wrap each expression in a `let v := …` to give it somewhere to land.

### `0x<hex>`

(`Expression::Literal`)

#### Description

A compile-time constant value with a declared type. New literals minted by the translator default to `Int(I256)`; passes that synthesize constants at narrower widths (e.g. a one-bit boolean from a constant comparison) attach the narrower type directly.

#### Syntax

```text
0x<hex>[: <type>]
```

#### Example

```text
let v0 := 0x2a              // 42 at the default i256
let v1 := 0x1: i1           // boolean true
let v2 := 0x80: i64         // narrowed by type inference
```

#### Operands

None — literals are leaves.

#### Result and purity

| Result | Purity |
|---|---|
| Same as the literal's `value_type` | Pure |

#### Annotations

| Source field | Printed as |
|---|---|
| `value: BigUint` | `0x<hex>` in the syntax position (not a comment annotation; it is the expression itself) |
| `value_type:` [`Type`](#type) | `: <type>` suffix when `value_type` is not the default `Int(I256)`; suppressed otherwise |

### `v<id>`

(`Expression::Var`)

#### Description

A reference to an existing SSA value, used as the entire right-hand side of a `let`. In a typical dump this is rare because the simplifier collapses `let v := v<id>` into the consumers of `v` via copy propagation; expect to see it only in dumps taken before simplification has run.

#### Syntax

```text
v<id>
```

#### Example

```text
let v5 := v3                // copy; usually eliminated by simplify
```

#### Operands

None — the expression is the value reference itself.

#### Result and purity

| Result | Purity |
|---|---|
| Same as the referenced value's type | Pure |

#### Annotations

None.

### `add`

(`Expression::Binary` with `BinaryOperation::Add`)

#### Description

Modular addition. Wraps on overflow; per EVM, the result is `(lhs + rhs) mod 2^N` where `N` is the operand width.

#### Syntax

```text
add($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := add(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | — |
| `rhs` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `widen_by_one(max(width(lhs), width(rhs)))` — one tier above the wider operand to account for the carry bit | Pure |

#### Annotations

None.

### `sub`

(`Expression::Binary` with `BinaryOperation::Sub`)

#### Description

Modular subtraction. Wraps on underflow; the result is `(lhs - rhs) mod 2^256` regardless of operand widths.

#### Syntax

```text
sub($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := sub(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | — |
| `rhs` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` — conservative; underflow on narrower operands could borrow into upper bits | Pure |

#### Annotations

None.

### `mul`

(`Expression::Binary` with `BinaryOperation::Mul`)

#### Description

Modular multiplication. The result is `(lhs * rhs) mod 2^256`.

#### Syntax

```text
mul($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := mul(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | — |
| `rhs` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `double_width(max(width(lhs), width(rhs)))` — the tier holding twice the wider operand's bits (skipping `i160` at the `i128` → `i256` transition) | Pure |

#### Annotations

None.

### `div`

(`Expression::Binary` with `BinaryOperation::Div`)

#### Description

Unsigned integer division. Per EVM, `div(x, 0) = 0` (no trap on division by zero).

#### Syntax

```text
div($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := div(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Dividend. |
| `rhs` | `i256` | Divisor; `0` yields a result of `0`, not a trap. |

#### Result and purity

| Result | Purity |
|---|---|
| `width(lhs)` — the quotient cannot exceed the dividend | Pure |

#### Annotations

None.

### `sdiv`

(`Expression::Binary` with `BinaryOperation::SDiv`)

#### Description

Signed two's-complement integer division. Per EVM, `sdiv(x, 0) = 0`; quotient is truncated toward zero.

#### Syntax

```text
sdiv($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := sdiv(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Dividend, treated as signed. |
| `rhs` | `i256` | Divisor, treated as signed; `0` yields `0`. |

#### Result and purity

| Result | Purity |
|---|---|
| `max(width(lhs), width(rhs))` — a negative divisor can push the result to full width | Pure |

#### Annotations

None.

### `mod`

(`Expression::Binary` with `BinaryOperation::Mod`)

#### Description

Unsigned modulo. Per EVM, `mod(x, 0) = 0`.

#### Syntax

```text
mod($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := mod(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Dividend. |
| `rhs` | `i256` | Divisor; `0` yields `0`. |

#### Result and purity

| Result | Purity |
|---|---|
| `width(lhs)` | Pure |

#### Annotations

None.

### `smod`

(`Expression::Binary` with `BinaryOperation::SMod`)

#### Description

Signed modulo. Per EVM, `smod(x, 0) = 0`; the result takes the sign of the dividend.

#### Syntax

```text
smod($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := smod(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Dividend, treated as signed. |
| `rhs` | `i256` | Divisor, treated as signed; `0` yields `0`. |

#### Result and purity

| Result | Purity |
|---|---|
| `width(lhs)` | Pure |

#### Annotations

None.

### `exp`

(`Expression::Binary` with `BinaryOperation::Exp`)

#### Description

Modular exponentiation: `(lhs ^ rhs) mod 2^256`. The most expensive arithmetic opcode in EVM (variable gas cost proportional to the byte length of `rhs`).

#### Syntax

```text
exp($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := exp(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Base. |
| `rhs` | `i256` | Exponent. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` — conservative; exponentiation can fill any width | Pure |

#### Annotations

None.

### `and`

(`Expression::Binary` with `BinaryOperation::And`)

#### Description

Bitwise AND. The common idiom for type narrowing: a constant mask on the right lets forward analysis pick up a tight result width.

#### Syntax

```text
and($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := and(v0, v1)
let v3: i8 := 0xff              // mask constant gets its own let-binding, narrowed to i8
let v4: i8 := and(v0, v3: i8)   // result narrows to i8 — AND can only clear bits
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | — |
| `rhs` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `min(width(lhs), width(rhs))` — AND can only clear bits, so the result fits in the narrower operand | Pure |

#### Annotations

None.

### `or`

(`Expression::Binary` with `BinaryOperation::Or`)

#### Description

Bitwise OR.

#### Syntax

```text
or($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := or(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | — |
| `rhs` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `max(width(lhs), width(rhs))` | Pure |

#### Annotations

None.

### `xor`

(`Expression::Binary` with `BinaryOperation::Xor`)

#### Description

Bitwise XOR.

#### Syntax

```text
xor($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := xor(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | — |
| `rhs` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `max(width(lhs), width(rhs))` | Pure |

#### Annotations

None.

### `shl`

(`Expression::Binary` with `BinaryOperation::Shl`)

#### Description

Logical left shift. Operand order follows EVM: `shl(shift, value)` computes `value << shift`. Shifts ≥ 256 produce `0`.

#### Syntax

```text
shl($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := shl(v0, v1)       // v1 shifted left by v0 bits
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Shift amount in bits. |
| `rhs` | `i256` | Value to shift. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` — conservative; bits may shift into any width | Pure |

#### Annotations

None.

### `shr`

(`Expression::Binary` with `BinaryOperation::Shr`)

#### Description

Logical right shift. Operand order follows EVM: `shr(shift, value)` computes `value >> shift` with zero-fill from the left. Shifts ≥ 256 produce `0`.

#### Syntax

```text
shr($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := shr(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Shift amount in bits. |
| `rhs` | `i256` | Value to shift. |

#### Result and purity

| Result | Purity |
|---|---|
| If `lhs` is a known constant `k`: tier holding `256 - k` bits (or `i1` for `k ≥ 256`). Otherwise: `width(rhs)`. | Pure |

#### Annotations

None.

### `sar`

(`Expression::Binary` with `BinaryOperation::Sar`)

#### Description

Arithmetic (signed) right shift. Operand order follows EVM: `sar(shift, value)` shifts `value` right by `shift` bits, preserving the sign bit. Shifts ≥ 256 saturate to `0` for non-negative values and to `-1` (all-ones) for negative values.

#### Syntax

```text
sar($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := sar(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Shift amount in bits. |
| `rhs` | `i256` | Value to shift, treated as signed. |

#### Result and purity

| Result | Purity |
|---|---|
| `width(rhs)` — unlike [`shr`](#shr), sign-extension means a constant shift cannot narrow the result | Pure |

#### Annotations

None.

### `lt`

(`Expression::Binary` with `BinaryOperation::Lt`)

#### Description

Unsigned less-than comparison. Returns `1` if `lhs < rhs`, else `0`.

#### Syntax

```text
lt($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := lt(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Compared unsigned. |
| `rhs` | `i256` | Compared unsigned. |

#### Result and purity

| Result | Purity |
|---|---|
| `i1` | Pure |

#### Annotations

None.

### `gt`

(`Expression::Binary` with `BinaryOperation::Gt`)

#### Description

Unsigned greater-than comparison. Returns `1` if `lhs > rhs`, else `0`.

#### Syntax

```text
gt($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := gt(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Compared unsigned. |
| `rhs` | `i256` | Compared unsigned. |

#### Result and purity

| Result | Purity |
|---|---|
| `i1` | Pure |

#### Annotations

None.

### `slt`

(`Expression::Binary` with `BinaryOperation::Slt`)

#### Description

Signed less-than comparison. Operands are treated as two's complement.

#### Syntax

```text
slt($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := slt(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Compared signed. |
| `rhs` | `i256` | Compared signed. |

#### Result and purity

| Result | Purity |
|---|---|
| `i1` | Pure |

#### Annotations

None.

### `sgt`

(`Expression::Binary` with `BinaryOperation::Sgt`)

#### Description

Signed greater-than comparison. Operands are treated as two's complement.

#### Syntax

```text
sgt($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := sgt(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Compared signed. |
| `rhs` | `i256` | Compared signed. |

#### Result and purity

| Result | Purity |
|---|---|
| `i1` | Pure |

#### Annotations

None.

### `eq`

(`Expression::Binary` with `BinaryOperation::Eq`)

#### Description

Equality comparison. Returns `1` if `lhs == rhs`, else `0`. Signedness is irrelevant.

#### Syntax

```text
eq($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := eq(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | — |
| `rhs` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `i1` | Pure |

#### Annotations

None.

### `byte`

(`Expression::Binary` with `BinaryOperation::Byte`)

#### Description

Extract a single byte from a 256-bit word. `byte(i, x)` returns the *i*-th byte of `x` with byte 0 being the most significant. If `i ≥ 32`, the result is `0`.

#### Syntax

```text
byte($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := byte(v0, v1)      // v0 = byte index, v1 = word
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Byte position; `0` = most significant byte. Values `≥ 32` yield `0`. |
| `rhs` | `i256` | Source word. |

#### Result and purity

| Result | Purity |
|---|---|
| `i8` | Pure |

#### Annotations

None.

### `signextend`

(`Expression::Binary` with `BinaryOperation::SignExtend`)

#### Description

Sign-extend an integer from a byte position. Per EVM, `signextend(b, x)` treats byte `b` of `x` as the most significant byte of a smaller signed integer and extends its sign through the upper bytes.

#### Syntax

```text
signextend($lhs[: <type>], $rhs[: <type>])
```

#### Example

```text
let v2 := signextend(v0, v1)  // v0 = byte position, v1 = value
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `lhs` | `i256` | Byte position of the sign byte (0–31). |
| `rhs` | `i256` | Source value. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` — the extended value occupies the full word | Pure |

#### Annotations

The width-targeted sign-extension primitive [`sext<i<bits>>`](#sextibits) (`Expression::SignExtendTo`) is a separate operation; see the bit-width conversions section.

### `addmod`

(`Expression::Ternary` with `BinaryOperation::AddMod`)

#### Description

Ternary modular addition: `(a + b) mod n`, computed without intermediate overflow. Per EVM, `n = 0` yields `0`.

#### Syntax

```text
addmod($a[: <type>], $b[: <type>], $n[: <type>])
```

#### Example

```text
let v3 := addmod(v0, v1, v2)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `a` | `i256` | First addend. |
| `b` | `i256` | Second addend. |
| `n` | `i256` | Modulus; `0` yields `0`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` — conservative | Pure |

#### Annotations

None.

### `mulmod`

(`Expression::Ternary` with `BinaryOperation::MulMod`)

#### Description

Ternary modular multiplication: `(a * b) mod n`, computed without intermediate overflow. Per EVM, `n = 0` yields `0`.

#### Syntax

```text
mulmod($a[: <type>], $b[: <type>], $n[: <type>])
```

#### Example

```text
let v3 := mulmod(v0, v1, v2)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `a` | `i256` | First factor. |
| `b` | `i256` | Second factor. |
| `n` | `i256` | Modulus; `0` yields `0`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` — conservative | Pure |

#### Annotations

None.

### `iszero`

(`Expression::Unary` with `UnaryOperation::IsZero`)

#### Description

Returns `1` if the operand is `0`, else `0`. Also serves as the logical NOT for boolean values.

#### Syntax

```text
iszero($operand[: <type>])
```

#### Example

```text
let v1 := iszero(v0)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `operand` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `i1` | Pure |

#### Annotations

None.

### `not`

(`Expression::Unary` with `UnaryOperation::Not`)

#### Description

Bitwise complement. Inverts every bit; equivalent to `xor(operand, 2^256 - 1)`.

#### Syntax

```text
not($operand[: <type>])
```

#### Example

```text
let v1 := not(v0)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `operand` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` — the complement fills the full word regardless of operand width | Pure |

#### Annotations

None.

### `clz`

(`Expression::Unary` with `UnaryOperation::Clz`)

#### Description

Count leading zeros. Returns the number of leading zero bits in the operand, where a value of `0` returns `256` (the full width). Not an EVM opcode; reaches newyork as a Yul builtin (`FunctionName::Clz`) and is translated directly by the Yul-to-newyork translator.

#### Syntax

```text
clz($operand[: <type>])
```

#### Example

```text
let v1 := clz(v0)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `operand` | `i256` | — |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` — in practice the value fits in nine bits (max `256`), so type inference often narrows further | Pure |

#### Annotations

None.

### `truncate<i<bits>>`

(`Expression::Truncate`)

#### Description

Reinterpret a wider integer as a narrower one by discarding the upper bits. The destination width is carried in the IR's `to: BitWidth` field and is rendered inside the angle brackets of the printer mnemonic. Narrowing-only; the source width must be greater than or equal to the destination width.

#### Syntax

```text
truncate<i<bits>>($value[: <type>])
```

#### Example

```text
let v1 := truncate<i64>(v0)
let v2 := truncate<i8>(v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `value` | `i256` | Source value; must be at least as wide as the destination. |

#### Result and purity

| Result | Purity |
|---|---|
| The destination width from the `to` field | Pure |

#### Annotations

None. The destination width is part of the operation name, not a debug annotation.

### `zext<i<bits>>`

(`Expression::ZeroExtend`)

#### Description

Reinterpret a narrower integer as a wider one by zero-filling the upper bits. The destination width is carried in the IR's `to: BitWidth` field. Widening-only.

#### Syntax

```text
zext<i<bits>>($value[: <type>])
```

#### Example

```text
let v1 := zext<i256>(v0: i8)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `value` | `i256` | Source value; must be no wider than the destination. |

#### Result and purity

| Result | Purity |
|---|---|
| The destination width from the `to` field | Pure |

#### Annotations

None.

### `sext<i<bits>>`

(`Expression::SignExtendTo`)

#### Description

Reinterpret a narrower signed integer as a wider one by sign-extending the high bit. The destination width is carried in the IR's `to: BitWidth` field. Distinct from [`signextend`](#signextend) (`Expression::Binary`), which is the EVM byte-position primitive; this one specifies the destination width directly and is introduced by passes that produce a sign-extended value at a known target width.

#### Syntax

```text
sext<i<bits>>($value[: <type>])
```

#### Example

```text
let v1 := sext<i256>(v0: i64)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `value` | `i256` | Source value; must be no wider than the destination. |

#### Result and purity

| Result | Purity |
|---|---|
| The destination width from the `to` field | Pure |

#### Annotations

None.

### `keccak256`

(`Expression::Keccak256`)

#### Description

Compute the Keccak-256 hash of `length` bytes of emulated EVM linear memory starting at `offset`. The general-purpose hashing primitive; the specialized variants below cover the common scratch-space patterns more compactly.

#### Syntax

```text
keccak256($offset[: <type>], $length[: <type>])
```

#### Example

```text
let v2 := keccak256(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `offset` | `i256` | Byte offset into linear memory; forward analysis widens to at least `i64`. |
| `length` | `i256` | Length of the region to hash, in bytes; forward analysis widens to at least `i64`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure — the hash is a deterministic function of the memory contents at evaluation time. Passes that hoist or dedupe must respect intervening memory writes. |

#### Annotations

None.

### `keccak256_pair`

(`Expression::Keccak256Pair`)

#### Description

Compound hash of two 256-bit words. Equivalent to `mstore(0, word0); mstore(32, word1); keccak256(0, 64)` but emitted as a single outlined call after `mem_opt`'s keccak fusion recognizes the pattern. The mapping-key idiom; see also [`mapping_sload`](#mapping_sload).

#### Syntax

```text
keccak256_pair($word0[: <type>], $word1[: <type>])
```

#### Example

```text
let v2 := keccak256_pair(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `word0` | `i256` | First word; the high 32 bytes of the hash input. |
| `word1` | `i256` | Second word; the low 32 bytes of the hash input. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `keccak256_single`

(`Expression::Keccak256Single`)

#### Description

Compound hash of a single 256-bit word. Equivalent to `mstore(0, word0); keccak256(0, 32)` but emitted as a single outlined call after `mem_opt`'s keccak fusion.

#### Syntax

```text
keccak256_single($word0[: <type>])
```

#### Example

```text
let v1 := keccak256_single(v0)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `word0` | `i256` | The word to hash. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `caller`

(`Expression::Caller`)

#### Description

Address of the immediate caller of the current call frame.

#### Syntax

```text
caller()
```

#### Example

```text
let v0 := caller()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i160` | Pure |

#### Annotations

None.

### `callvalue`

(`Expression::CallValue`)

#### Description

Value (wei) attached to the current call.

#### Syntax

```text
callvalue()
```

#### Example

```text
let v0 := callvalue()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `origin`

(`Expression::Origin`)

#### Description

Address of the original externally owned account that initiated the transaction.

#### Syntax

```text
origin()
```

#### Example

```text
let v0 := origin()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i160` | Pure |

#### Annotations

None.

### `address`

(`Expression::Address`)

#### Description

Address of the contract executing the current call frame.

#### Syntax

```text
address()
```

#### Example

```text
let v0 := address()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i160` | Pure |

#### Annotations

None.

### `chainid`

(`Expression::ChainId`)

#### Description

Chain identifier of the network the contract is executing on.

#### Syntax

```text
chainid()
```

#### Example

```text
let v0 := chainid()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `gas`

(`Expression::Gas`)

#### Description

Remaining gas at the point of evaluation. Modeled as a pure expression for IR purposes; in practice it changes between evaluations, so any simplifier that deduplicates pure expressions must respect `gas` as a barrier.

#### Syntax

```text
gas()
```

#### Example

```text
let v0 := gas()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure (per IR; see Description) |

#### Annotations

None.

### `msize`

(`Expression::MSize`)

#### Description

Highest byte offset of emulated EVM linear memory that has been touched, rounded up to the next 32-byte boundary. Unlike [`gas`](#gas), classified as side-effectful by the simplifier: unused `msize()` bindings are not eliminated, because the result depends on the program's memory-access history and would change if the surrounding statements were reordered.

#### Syntax

```text
msize()
```

#### Example

```text
let v0 := msize()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Effectful (see Description) |

#### Annotations

None.

### `coinbase`

(`Expression::Coinbase`)

#### Description

Address of the block's coinbase (block author).

#### Syntax

```text
coinbase()
```

#### Example

```text
let v0 := coinbase()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i160` | Pure |

#### Annotations

None.

### `timestamp`

(`Expression::Timestamp`)

#### Description

Block timestamp, as a Unix epoch second.

#### Syntax

```text
timestamp()
```

#### Example

```text
let v0 := timestamp()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure |

#### Annotations

None.

### `number`

(`Expression::Number`)

#### Description

Current block number.

#### Syntax

```text
number()
```

#### Example

```text
let v0 := number()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure |

#### Annotations

None.

### `difficulty`

(`Expression::Difficulty`)

#### Description

Pre-merge block difficulty. On post-merge chains this is the block's `prevrandao` value.

#### Syntax

```text
difficulty()
```

#### Example

```text
let v0 := difficulty()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `gaslimit`

(`Expression::GasLimit`)

#### Description

Block gas limit.

#### Syntax

```text
gaslimit()
```

#### Example

```text
let v0 := gaslimit()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure |

#### Annotations

None.

### `basefee`

(`Expression::BaseFee`)

#### Description

Current block's EIP-1559 base fee per gas.

#### Syntax

```text
basefee()
```

#### Example

```text
let v0 := basefee()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `blobbasefee`

(`Expression::BlobBaseFee`)

#### Description

Current block's EIP-4844 blob base fee per gas.

#### Syntax

```text
blobbasefee()
```

#### Example

```text
let v0 := blobbasefee()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `blobhash`

(`Expression::BlobHash`)

#### Description

Versioned hash of the blob at the given index in the current transaction's blob list.

#### Syntax

```text
blobhash($index[: <type>])
```

#### Example

```text
let v1 := blobhash(v0)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `index` | `i256` | Blob index; forward analysis widens to at least `i64`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `blockhash`

(`Expression::BlockHash`)

#### Description

Hash of the block with the given number. Per EVM, valid only for the most recent 256 blocks; outside that range the result is `0`.

#### Syntax

```text
blockhash($number[: <type>])
```

#### Example

```text
let v1 := blockhash(v0)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `number` | `i256` | Block number; forward analysis widens to `i256`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `selfbalance`

(`Expression::SelfBalance`)

#### Description

Balance (in wei) of the contract executing the current call frame. Cheaper than `balance(address())`.

#### Syntax

```text
selfbalance()
```

#### Example

```text
let v0 := selfbalance()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `gasprice`

(`Expression::GasPrice`)

#### Description

Effective gas price of the current transaction.

#### Syntax

```text
gasprice()
```

#### Example

```text
let v0 := gasprice()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `calldataload`

(`Expression::CallDataLoad`)

#### Description

Read 32 bytes from the current call's calldata at the given offset. Reads past the end of calldata return zero bytes.

#### Syntax

```text
calldataload($offset[: <type>])
```

#### Example

```text
let v1 := calldataload(v0)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `offset` | `i256` | Byte offset into calldata. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `calldatasize`

(`Expression::CallDataSize`)

#### Description

Length of the current call's calldata, in bytes.

#### Syntax

```text
calldatasize()
```

#### Example

```text
let v0 := calldatasize()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure |

#### Annotations

None.

### `returndatasize`

(`Expression::ReturnDataSize`)

#### Description

Length of the most recently returned data buffer from a sub-call, in bytes. Modeled as pure per IR but reflects the last `ExternalCall` / `Create` result; consumers must respect that ordering.

#### Syntax

```text
returndatasize()
```

#### Example

```text
let v0 := returndatasize()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure (per IR; see Description) |

#### Annotations

None.

### `codesize`

(`Expression::CodeSize`)

#### Description

Size of the currently executing code, in bytes.

#### Syntax

```text
codesize()
```

#### Example

```text
let v0 := codesize()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure |

#### Annotations

None.

### `extcodesize`

(`Expression::ExtCodeSize`)

#### Description

Size of the code deployed at the given address, in bytes. Returns `0` for accounts with no deployed code.

#### Syntax

```text
extcodesize($address[: <type>])
```

#### Example

```text
let v1 := extcodesize(v0: i160)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `address` | `i256` | Account address; forward analysis widens to at least `i160`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure |

#### Annotations

None.

### `extcodehash`

(`Expression::ExtCodeHash`)

#### Description

Keccak-256 hash of the code deployed at the given address. Returns `0` for non-existent accounts.

#### Syntax

```text
extcodehash($address[: <type>])
```

#### Example

```text
let v1 := extcodehash(v0: i160)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `address` | `i256` | Account address; forward analysis widens to at least `i160`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `balance`

(`Expression::Balance`)

#### Description

Balance (in wei) of the given account address. Use [`selfbalance`](#selfbalance) for the contract executing the current call frame (cheaper).

#### Syntax

```text
balance($address[: <type>])
```

#### Example

```text
let v1 := balance(v0: i160)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `address` | `i256` | Account address; forward analysis widens to at least `i160`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

None.

### `mload`

(`Expression::MLoad`)

#### Description

Read a 32-byte word from emulated EVM linear memory at `offset`. The word is read big-endian per EVM semantics. Pure per IR, but reads after writes return the new value; the memory passes track read/write dependencies separately.

#### Syntax

```text
mload($offset[: <type>]) [/* <region> */]
```

#### Example

```text
let v1 := mload(v0)
let v2 := mload(v3) /* free_ptr */
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `offset` | `i256` | Byte offset into linear memory; forward analysis widens to at least `i64`. |

#### Result and purity

| Result | Purity |
|---|---|
| `i32` when region is `FreePointerSlot`; `i256` otherwise | Pure (per IR; see Description) |

#### Annotations

| Source field | Printed as |
|---|---|
| `region:` [`MemoryRegion`](#memoryregion) | `/* scratch */` · `/* free_ptr */` · `/* dynamic */` (`Unknown` is suppressed) |

Same tagging rules as [`mstore`](#mstore). The region also determines the result width: a load from `FreePointerSlot` produces an `i32` since the FMP fits in a pointer-sized word.

### `sload`

(`Expression::SLoad`)

#### Description

Read a 32-byte word from persistent contract storage at the given key. Pure per IR; reads after writes to the same slot return the new value.

#### Syntax

```text
sload($key[: <type>]) [/* slot: 0x<hex> */]
```

#### Example

```text
let v1 := sload(v0)
let v2 := sload(v3) /* slot: 0x0 */
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `key` | `i256` | Storage slot. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure (per IR; see Description) |

#### Annotations

| Source field | Printed as |
|---|---|
| `static_slot: Option<BigUint>` | `/* slot: 0x<hex> */` when set; suppressed otherwise |

Same tagging rules as [`sstore`](#sstore). The printer renders the annotation whenever the field is `Some` and the deduplicator's canonicalizer partitions signatures by slot; no pass currently writes `Some(...)`, however, so in present-day dumps the annotation is dormant.

### `tload`

(`Expression::TLoad`)

#### Description

Read a 32-byte word from transient storage at the given key. Transient storage is wiped at the end of the transaction; pair with [`tstore`](#tstore).

#### Syntax

```text
tload($key[: <type>])
```

#### Example

```text
let v1 := tload(v0)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `key` | `i256` | Transient storage slot. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure (per IR; see Description) |

#### Annotations

None. The IR does not track a static slot for `tload`.

### `mapping_sload`

(`Expression::MappingSLoad`)

#### Description

Compound load for a Solidity mapping element. Equivalent to `mstore(0, key); mstore(32, slot); sload(keccak256(0, 64))` but emitted as a single outlined call after the `mapping_access_outlining` pass recognizes the pattern (it fuses a `keccak256_pair` — itself produced by `mem_opt`'s keccak fusion — followed by an `sload` whose key has a single consumer). Only valid when the intermediate hash is used exclusively by this load.

#### Syntax

```text
mapping_sload($key[: <type>], $slot[: <type>])
```

#### Example

```text
let v2 := mapping_sload(v0: i160, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `key` | `i256` | Mapping key; often narrowed to `i160` for address keys. |
| `slot` | `i256` | The mapping's declared storage slot. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure (per IR; see Description) |

#### Annotations

None. The fused statement's effective storage slot is the keccak hash of the key and the declared slot, which is never a compile-time constant; no `static_slot` hint is surfaced.

### `dataoffset`

(`Expression::DataOffset`)

#### Description

Offset of a named data segment within the deployed code. The identifier is a string carried in the IR's `id: String` field; the linker resolves it to a concrete offset.

#### Syntax

```text
dataoffset("<id>")
```

#### Example

```text
let v0 := dataoffset("MyContract_deployed")
```

#### Operands

None — the identifier is a quoted string literal in the syntax position, not an operand.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

| Source field | Printed as |
|---|---|
| `id: String` | The quoted identifier in the syntax position (not a comment annotation; it is the expression itself). |

### `datasize`

(`Expression::DataSize`)

#### Description

Size of a named data segment within the deployed code, in bytes. The identifier is resolved by the linker.

#### Syntax

```text
datasize("<id>")
```

#### Example

```text
let v0 := datasize("MyContract_deployed")
```

#### Operands

None — the identifier is a quoted string literal in the syntax position, not an operand.

#### Result and purity

| Result | Purity |
|---|---|
| `i64` | Pure |

#### Annotations

| Source field | Printed as |
|---|---|
| `id: String` | The quoted identifier in the syntax position. |

### `loadimmutable`

(`Expression::LoadImmutable`)

#### Description

Read the value of a named immutable variable. Immutables are written once during contract construction by `SetImmutable` and read afterwards via this expression.

#### Syntax

```text
loadimmutable("<key>")
```

#### Example

```text
let v0 := loadimmutable("MyContract.owner")
```

#### Operands

None — the key is a quoted string literal in the syntax position.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` | Pure |

#### Annotations

| Source field | Printed as |
|---|---|
| `key: String` | The quoted identifier in the syntax position. |

### `linkersymbol`

(`Expression::LinkerSymbol`)

#### Description

Address of an external library, resolved by the linker. The path encodes the library's source location and identifier.

#### Syntax

```text
linkersymbol("<path>")
```

#### Example

```text
let v0 := linkersymbol("contracts/Library.sol:L")
```

#### Operands

None — the path is a quoted string literal in the syntax position.

#### Result and purity

| Result | Purity |
|---|---|
| `i160` | Pure |

#### Annotations

| Source field | Printed as |
|---|---|
| `path: String` | The quoted path in the syntax position. |

### `<func_name>`

(`Expression::Call`; the printer emits `func_<id>` when no function name is registered)

#### Description

Internal function call. Invokes a user-defined function declared earlier in the same object; the mnemonic is the function's Yul-level name, or `func_<id>` if the printer has no name registered for the `FunctionId`. Distinct from [`call`](#call) and the other EVM call-opcode statements, which cross the contract boundary.

#### Syntax

```text
<func_name>([$argument_0[: <type>], $argument_1[: <type>], …])
```

#### Example

```text
let v3 := abi_decode_uint256(v0, v1, v2)
let v4, v5 := returns_two(v0)           // multi-return via let multi-binding
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `arguments` | `Vec<Value>` | Zero or more argument values, in declaration order; each operand may carry a `: <type>` suffix. |

#### Result and purity

| Result | Purity |
|---|---|
| One or more values, widths taken from the callee's declared return types (or the inferred return widths, narrowed via the interprocedural pass). Falls back to `i256` when the callee's returns are unknown to type inference. | Effectful — the simplifier treats every call as side-effectful regardless of callee body, so unused call bindings are not DCE'd. The transitive purity of the callee is not tracked at the IR level. |

#### Annotations

| Source field | Printed as |
|---|---|
| `function: FunctionId` | The callee's name in the syntax position (or `func_<id>` if the printer has no name registered). |

## Memory and storage writes

The operations in this section all modify external state: emulated EVM linear memory, persistent storage, or transient storage. They are statements (not expressions) and they are never pure. Simplification and deduplication never reorder them with respect to each other or with respect to reverts; the memory passes treat them as the side-effect boundary for their analyses.

### `mstore`

(`Statement::MStore`)

#### Description

Write a 32-byte word to emulated EVM linear memory at `offset`. The word is stored big-endian, matching EVM semantics; the codegen handles the byte swap on PolkaVM's little-endian RISC-V target.

#### Syntax

```text
mstore($offset[: <type>], $value[: <type>]) [/* <region> */]
```

#### Example

```text
mstore(v0, v1)                    // Unknown region; no annotation printed
mstore(v2, v3) /* scratch */      // offset proven to land in 0x00..0x3f
mstore(v4, v5) /* free_ptr */     // offset is exactly 0x40
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `offset` | `i256` | Byte offset into linear memory; forward analysis widens to at least `i64`. |
| `value` | `i256` | The 32-byte word to store. Narrower values are zero-extended at codegen time. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

| Source field | Printed as |
|---|---|
| `region:` [`MemoryRegion`](#memoryregion) | `/* scratch */` · `/* free_ptr */` · `/* dynamic */` (`Unknown` is suppressed) |

Assigned at translation time from the constant offset (if any); consumed by mem_opt, FMP propagation, and byte-swap mode selection.

### `mstore8`

(`Statement::MStore8`)

#### Description

Write a single byte to emulated EVM linear memory at `offset`. The low 8 bits of `value` are stored; the upper bits are ignored. The operation is otherwise identical to `mstore`: same operand shape, same region tag, same side-effect classification.

#### Syntax

```text
mstore8($offset[: <type>], $value[: <type>]) [/* <region> */]
```

#### Example

```text
mstore8(v0, v1: i8)             // value narrowed to i8 by type inference
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `offset` | `i256` | Byte offset into linear memory; forward analysis widens to at least `i64`. |
| `value` | `i256` | Only the low 8 bits are stored. Often narrowed to `i8` by type inference. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

| Source field | Printed as |
|---|---|
| `region:` [`MemoryRegion`](#memoryregion) | `/* scratch */` · `/* free_ptr */` · `/* dynamic */` (`Unknown` is suppressed) |

Same tagging rules as [`mstore`](#mstore). Most `mstore8`s carry an `Unknown` region in practice because single-byte writes typically target offsets the translator cannot prove constant.

### `mcopy`

(`Statement::MCopy`)

#### Description

Copy `length` bytes from `src` to `dest` within emulated EVM linear memory. The Yul builtin `mcopy` maps directly onto this statement; unlike `mstore`, it does not carry a region tag because the source and destination ranges may straddle multiple regions.

#### Syntax

```text
mcopy($dest[: <type>], $src[: <type>], $length[: <type>])
```

#### Example

```text
mcopy(v0, v1, v2)               // dest, src, length
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `dest` | `i256` | Destination byte offset in linear memory. |
| `src` | `i256` | Source byte offset in linear memory. |
| `length` | `i256` | Number of bytes to copy. Overlapping ranges follow EVM-defined memmove semantics. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

None. `mcopy` carries no region tag because the source and destination ranges may straddle multiple regions, and no static-slot hint because the copy is not storage-bound.

### `sstore`

(`Statement::SStore`)

#### Description

Write a 32-byte word to persistent contract storage at `key`. The operation is the durable counterpart of `mstore`: the value survives across transactions and is observable to subsequent calls to the contract.

#### Syntax

```text
sstore($key[: <type>], $value[: <type>]) [/* slot: 0x<hex> */]
```

#### Example

```text
sstore(v0, v1)
sstore(v2, v3) /* slot: 0x0 */
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `key` | `i256` | Storage slot. May be a constant slot, a keccak-derived slot for mappings or dynamic arrays, or an arbitrary expression. |
| `value` | `i256` | The 256-bit word to store. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

| Source field | Printed as |
|---|---|
| `static_slot: Option<BigUint>` | `/* slot: 0x<hex> */` when set; suppressed otherwise |

The printer renders the annotation whenever the field is `Some`, and the deduplicator's canonicalizer and mapping-fusion analyses consume it as part of the signature. No pass currently writes `Some(...)`, so the annotation is dormant in present-day dumps; when absent, alias and dedup analyses fall back to the conservative "may alias any slot" assumption.

### `tstore`

(`Statement::TStore`)

#### Description

Write a 32-byte word to transient storage at `key`. Transient storage is wiped at the end of the transaction, so `tstore` is the right primitive for per-transaction bookkeeping (reentrancy guards, cached results) without the gas cost of `sstore` on EVM. On PolkaVM the transient backing store is provided by pallet-revive.

#### Syntax

```text
tstore($key[: <type>], $value[: <type>])
```

#### Example

```text
tstore(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `key` | `i256` | Transient storage slot. |
| `value` | `i256` | The 256-bit word to store. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

None. Unlike `sstore`, the IR does not track a static slot for `tstore`: transient storage's short-lived lifetime makes the slot-aware optimizations less valuable, and the translator does not produce the annotation.

### `mapping_sstore`

(`Statement::MappingSStore`)

#### Description

Compound store for a Solidity mapping element. Equivalent to `mstore(0, key); mstore(32, slot); sstore(keccak256(0, 64), value)` but emitted as a single outlined statement after the `mapping_access_outlining` pass recognizes the pattern (it fuses a `keccak256_pair` followed by an `sstore` whose key has a single consumer). Only valid when the intermediate hash is not observed by any other statement.

#### Syntax

```text
mapping_sstore($key[: <type>], $slot[: <type>], $value[: <type>])
```

#### Example

```text
mapping_sstore(v0: i160, v1, v2)        // address key, declared slot, value
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `key` | `i256` | Mapping key. Often narrowed to `i160` for address keys. |
| `slot` | `i256` | The mapping's declared storage slot. Typically a small constant. |
| `value` | `i256` | The value to store at the computed storage location. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

None. `mapping_sstore` deliberately drops the `static_slot` annotation that the original `sstore` may have carried, because the fused statement's effective slot is the keccak hash of the key and the declared slot, which is never a compile-time constant.

## Bulk copies

Multi-byte memory copies from the EVM-accessible byte sources (code, external code, returndata, embedded data, and calldata) into emulated EVM linear memory. They all take the same shape: a destination memory offset, a source offset, and a length. They are effectful and act as opaque barriers to the memory passes.

### `codecopy`

(`Statement::CodeCopy`)

#### Description

Copy `length` bytes from the currently executing code at `offset` into emulated EVM linear memory at `dest`. Reads past the end of code yield zero bytes.

#### Syntax

```text
codecopy($dest[: <type>], $offset[: <type>], $length[: <type>])
```

#### Example

```text
codecopy(v0, v1, v2)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `dest` | `i256` | Destination byte offset in linear memory. |
| `offset` | `i256` | Source byte offset in the executing code. |
| `length` | `i256` | Number of bytes to copy. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

None.

### `extcodecopy`

(`Statement::ExtCodeCopy`)

#### Description

Copy `length` bytes from the code at `address` starting at `offset` into emulated EVM linear memory at `dest`. Reads beyond the code yield zero bytes; non-existent accounts yield all zeros.

#### Syntax

```text
extcodecopy($address[: <type>], $dest[: <type>], $offset[: <type>], $length[: <type>])
```

#### Example

```text
extcodecopy(v0: i160, v1, v2, v3)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `address` | `i256` | Account whose code to read; narrows to `i160`. |
| `dest` | `i256` | Destination byte offset in linear memory. |
| `offset` | `i256` | Source byte offset in the external code. |
| `length` | `i256` | Number of bytes to copy. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

None.

### `returndatacopy`

(`Statement::ReturnDataCopy`)

#### Description

Copy `length` bytes from the most recent sub-call's return data starting at `offset` into emulated EVM linear memory at `dest`. Per EVM, reads past the return data's end revert; the memory passes treat this as a potential trap site.

#### Syntax

```text
returndatacopy($dest[: <type>], $offset[: <type>], $length[: <type>])
```

#### Example

```text
returndatacopy(v0, v1, v2)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `dest` | `i256` | Destination byte offset in linear memory. |
| `offset` | `i256` | Source byte offset in the return-data buffer. |
| `length` | `i256` | Number of bytes to copy. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful (may revert on out-of-range reads, per EVM) |

#### Annotations

None.

### `datacopy`

(`Statement::DataCopy`)

#### Description

Copy `length` bytes from an embedded data segment starting at `offset` into emulated EVM linear memory at `dest`. The source segment is resolved by the linker, typically used to pull constants compiled into the bytecode into runtime memory.

#### Syntax

```text
datacopy($dest[: <type>], $offset[: <type>], $length[: <type>])
```

#### Example

```text
datacopy(v0, v1, v2)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `dest` | `i256` | Destination byte offset in linear memory. |
| `offset` | `i256` | Source byte offset in the data segment. |
| `length` | `i256` | Number of bytes to copy. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

None.

### `calldatacopy`

(`Statement::CallDataCopy`)

#### Description

Copy `length` bytes from the current call's calldata starting at `offset` into emulated EVM linear memory at `dest`. Reads past the end of calldata yield zero bytes.

#### Syntax

```text
calldatacopy($dest[: <type>], $offset[: <type>], $length[: <type>])
```

#### Example

```text
calldatacopy(v0, v1, v2)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `dest` | `i256` | Destination byte offset in linear memory. |
| `offset` | `i256` | Source byte offset in calldata. |
| `length` | `i256` | Number of bytes to copy. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

None.

## Bindings and wrappers

The statements that bind SSA values, hold loose expressions evaluated for their side effects, and write to immutable storage. Every pure expression in this reference's earlier sections appears on the right-hand side of one of these statements (almost always `let`).

### `let`

(`Statement::Let`)

#### Description

SSA binding: evaluate an expression and bind its result(s) to a list of fresh value ids. The `let` statement is the only mechanism by which pure expressions enter the value namespace; every `v<id>` in a dump was produced by a `let` (or by a value-yielding control-flow statement or by a parameter at function entry).

#### Syntax

```text
let $binding_0[, $binding_1, …] := $expression
```

#### Example

```text
let v3 := add(v0, v1)
let v4, v5 := if v2 [v0, v1] { … } else { … }   // multi-binding from a value-yielding If
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `bindings` | `Vec<ValueId>` | One or more fresh SSA ids to bind. Most expressions produce one value; control-flow statements may produce several. |
| `value` | `Expression` | The right-hand side; see any of the Pure expression entries. |

#### Result and purity

| Result | Purity |
|---|---|
| None directly — the bound ids carry the expression's result(s) | Effectful (binding establishment); the right-hand side's purity is independent |

#### Annotations

None.

### Expression statement

(`Statement::Expression`)

#### Description

Wraps an expression evaluated for its observable consequences but whose value is not bound. Typically a user-defined function call (`Expression::Call`) whose return values the source code discarded, or another Yul expression statement that does not have a dedicated `Statement::` variant. EVM external calls (`call`, `delegatecall`, etc.) and contract creation (`create`, `create2`) translate to dedicated `Statement::ExternalCall` and `Statement::Create` variants, not through this wrapper.

#### Syntax

```text
$expression
```

#### Example

```text
keccak256(v0, v1)           // hash computed but not bound to a value
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `expression` | `Expression` | Any expression; result is discarded. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful (per its statement position) |

#### Annotations

None.

### `setimmutable`

(`Statement::SetImmutable`)

#### Description

Write an immutable variable during contract construction. Immutables are written once in the constructor and read later via [`loadimmutable`](#loadimmutable). The key is a string identifier resolved by the linker.

#### Syntax

```text
setimmutable("<key>", $value[: <type>])
```

#### Example

```text
setimmutable("MyContract.owner", v0: i160)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `value` | `i256` | The value to store; the key is a quoted string literal in the syntax position. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

| Source field | Printed as |
|---|---|
| `key: String` | The quoted identifier in the syntax position. |

## Structured control flow

The IR's control flow is structured: `if`, `switch`, and `for` are statements with explicit nested regions, each carrying input values and yielding output values. The jump-like statements (`break`, `continue`, `leave`) are scoped to their nearest enclosing construct. Nested blocks create lexical scope without otherwise changing control flow.

### `if`

(`Statement::If`)

#### Description

Conditional execution with optional value yields. The `then` region runs when `condition` is non-zero; the `else` region runs otherwise. If `outputs` is non-empty, both regions must yield the same number of values and the statement is bound by a `let`.

#### Syntax

```text
if $condition[: <type>] [[$input_0, $input_1, …]] { … } [else { … }]
```

#### Example

```text
if v0 {
    sstore(v1, v2)
}

let v5, v6 := if v3 [v1, v2] {
    let v7 := add(v2, 0x1)
    yield v1, v7
} else {
    yield v1, v2
}
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `condition` | `i256` | Branch selector; non-zero takes the `then` region. Often narrowed to `i1`. |
| `inputs` | `Vec<Value>` | Values threaded into both regions, printed in square brackets after the condition. |
| (regions) | — | The `then_region` is mandatory; the `else_region` is optional and, when absent, implicitly yields the inputs unchanged. |

#### Result and purity

| Result | Purity |
|---|---|
| None for the statement form; for the value-yielding form, one value per `outputs` binding, types taken from the yielded values | Effectful (control flow) |

#### Annotations

None.

### `switch`

(`Statement::Switch`)

#### Description

Multi-way dispatch on a scrutinee value. Each case matches a specific constant and runs its region; an optional `default` region catches non-matching values. Like `if`, switch may yield values via `outputs` and accept thread-through values via `inputs`.

#### Syntax

```text
switch $scrutinee[: <type>] [[$input_0, …]]
case 0x<hex> {
    …
}
[case 0x<hex> {
    …
} …]
[default {
    …
}]
```

#### Example

```text
switch v0
case 0x0 {
    sstore(v1, v2)
}
case 0x1 {
    sstore(v1, v3)
}
default {
    invalid()
}
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `scrutinee` | `i256` | The value to compare against each case. |
| `inputs` | `Vec<Value>` | Values threaded into every case and default region. |
| `cases` | `Vec<SwitchCase>` | Each case carries a constant `value: BigUint` and a region. |
| (default) | — | Optional fall-through region. |

#### Result and purity

| Result | Purity |
|---|---|
| None for the statement form; one value per `outputs` binding for the value-yielding form | Effectful (control flow) |

#### Annotations

None.

### `for`

(`Statement::For`)

#### Description

Structured loop with explicit loop-carried variables. Each iteration evaluates `condition_statements` followed by `condition`; if the condition is non-zero, the `body` region runs, then the `post` region runs, and the loop iterates. Loop-carried variables are passed as SSA values through each region. `break` exits the loop and `continue` jumps to the post region.

#### Syntax

```text
for { $variable_0 := $initial_0[, …] }
    [// condition statements:
        …]
    condition: $condition
    post [($post_input_variable_0[, …])] {
        …
    }
    body {
        … body …
    }
```

#### Example

```text
for { v1 := 0x0 }
    condition: lt(v1, 0xa)
    post (v3) {
        let v4 := add(v1, 0x1)
        yield v4
    }
    body {
        sstore(v1, v2)
        yield v1
    }
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `initial_values` | `Vec<Value>` | Starting values for the loop-carried variables. |
| `loop_variables` | `Vec<ValueId>` | SSA ids visible inside condition, body, and post. |
| `condition_statements` | `Vec<Statement>` | Statements evaluated each iteration *before* the condition expression; emitted into the loop header block. Printed only when non-empty, behind a `// condition statements:` comment. |
| `condition` | `Expression` | Re-evaluated each iteration; non-zero continues, zero exits. |
| `body` | `Region` | Loop body; yields current loop-carried values. |
| `post_input_variables` | `Vec<ValueId>` | Input SSA ids for the post region (one per loop-carried variable); receive the body's yielded values merged with continue-site values via phi nodes in the LLVM codegen. |
| `post` | `Region` | Runs after each body iteration (and after `continue`); yields updated loop-carried values. |
| `outputs` | `Vec<ValueId>` | Final loop-carried values after exit. |

#### Result and purity

| Result | Purity |
|---|---|
| None for the statement form; one value per `outputs` binding for the value-yielding form | Effectful (control flow) |

#### Annotations

None.

### `break`

(`Statement::Break`)

#### Description

Exit the innermost enclosing `for` loop. Carries the current values of loop-carried variables at the break point; these become the loop's outputs.

#### Syntax

```text
break
```

#### Example

```text
if v0 { break [v1, v2] }
```

#### Operands

The loop-carried `values: Vec<Value>` print in brackets when non-empty (e.g. `break [v1, v2]`).

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful (control flow) |

#### Annotations

None.

### `continue`

(`Statement::Continue`)

#### Description

Skip to the post region of the innermost enclosing `for` loop. Like `break`, carries the current values of loop-carried variables internally.

#### Syntax

```text
continue
```

#### Example

```text
if v0 { continue [v1, v2] }
```

#### Operands

The loop-carried `values` print in brackets when non-empty (e.g. `continue [v1, v2]`).

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful (control flow) |

#### Annotations

None.

### `leave`

(`Statement::Leave`)

#### Description

Exit the current function, returning the listed values as the function's return values. The Yul-level `leave` keyword translates directly to this statement; the inlining pass eliminates intra-function `leave`s where possible via the exit-flag transformation.

#### Syntax

```text
leave [[$value_0[: <type>], $value_1[: <type>], …]]
```

#### Example

```text
leave [v0, v1]              // returns v0 and v1 from the function
leave                       // returns nothing (void function)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `return_values` | `Vec<Value>` | Empty for void functions; otherwise one entry per declared return. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful (control flow) |

#### Annotations

None.

### Nested block

(`Statement::Block`)

#### Description

A lexical scope without conditional or iterative behavior. The body is a region; control falls through after the region's statements complete. Used to bound the visibility of inner bindings.

#### Syntax

```text
{
    …
}
```

#### Example

```text
{
    let v0 := add(v1, v2)
    sstore(v3, v0)
}                           // v0 is no longer in scope here
```

#### Operands

None — the body is a region, not an operand.

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful (per the body's contents) |

#### Annotations

None.

## External interaction

Statements that cross the contract boundary: external calls, contract creation, and event log emission. All produce or rely on external state and act as barriers to memory and storage analyses.

### `call`

(`Statement::ExternalCall` with `CallKind::Call`)

#### Description

Standard external call that may transfer value. Reads `args_length` bytes from emulated EVM linear memory at `args_offset` as calldata, executes the target, and writes up to `ret_length` bytes of return data into linear memory at `ret_offset`. The boolean result indicates success.

#### Syntax

```text
let $result := call($gas[: <type>], $address[: <type>], $value[: <type>], $args_offset[: <type>], $args_length[: <type>], $ret_offset[: <type>], $ret_length[: <type>])
```

#### Example

```text
let v8 := call(v0, v1: i160, v2, v3, v4, v5, v6)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `gas` | `i256` | Gas to forward to the target. |
| `address` | `i256` | Callee address; narrows to `i160`. |
| `value` | `i256` | Wei to transfer with the call. |
| `args_offset` | `i256` | Calldata source offset in linear memory. |
| `args_length` | `i256` | Calldata length in bytes. |
| `ret_offset` | `i256` | Return-data destination offset in linear memory. |
| `ret_length` | `i256` | Maximum return-data length. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` (success flag: `1` on success, `0` on revert/error; narrowable to `i1`) | Effectful |

#### Annotations

None.

### `callcode`

(`Statement::ExternalCall` with `CallKind::CallCode`)

#### Description

Deprecated EVM opcode that executes the callee's code in the caller's context but with the callee's storage. Not supported by the newyork backend (codegen rejects it); use [`delegatecall`](#delegatecall) instead.

#### Syntax

```text
let $result := callcode($gas[: <type>], $address[: <type>], $value[: <type>], $args_offset[: <type>], $args_length[: <type>], $ret_offset[: <type>], $ret_length[: <type>])
```

#### Example

```text
let v8 := callcode(v0, v1: i160, v2, v3, v4, v5, v6)
```

#### Operands

Same shape as [`call`](#call).

#### Result and purity

| Result | Purity |
|---|---|
| `i256` (success flag; narrowable to `i1`) | Effectful |

#### Annotations

None.

### `delegatecall`

(`Statement::ExternalCall` with `CallKind::DelegateCall`)

#### Description

Execute the callee's code in the caller's context: same storage, same sender, same call value. The standard mechanism for library calls and proxy patterns. No `value` operand (the caller's call value is inherited).

#### Syntax

```text
let $result := delegatecall($gas[: <type>], $address[: <type>], $args_offset[: <type>], $args_length[: <type>], $ret_offset[: <type>], $ret_length[: <type>])
```

#### Example

```text
let v7 := delegatecall(v0, v1: i160, v2, v3, v4, v5)
```

#### Operands

Same shape as [`call`](#call) minus the `value` operand.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` (success flag; narrowable to `i1`) | Effectful |

#### Annotations

None.

### `staticcall`

(`Statement::ExternalCall` with `CallKind::StaticCall`)

#### Description

Read-only external call. Any state modification in the callee (including nested calls) causes the call to revert. No `value` operand.

#### Syntax

```text
let $result := staticcall($gas[: <type>], $address[: <type>], $args_offset[: <type>], $args_length[: <type>], $ret_offset[: <type>], $ret_length[: <type>])
```

#### Example

```text
let v7 := staticcall(v0, v1: i160, v2, v3, v4, v5)
```

#### Operands

Same shape as [`call`](#call) minus the `value` operand.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` (success flag; narrowable to `i1`) | Effectful (no state writes, but still an external boundary and may revert) |

#### Annotations

None.

### `create`

(`Statement::Create` with `CreateKind::Create`)

#### Description

Deploy a new contract with the given init-code bytes, transferring `value` wei from the caller. The new contract's address is derived from the caller's address and nonce; on failure the result is `0`.

#### Syntax

```text
let $result := create($value[: <type>], $offset[: <type>], $length[: <type>])
```

#### Example

```text
let v4 := create(v0, v1, v2)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `value` | `i256` | Wei to transfer to the new contract. |
| `offset` | `i256` | Linear-memory offset of the init code. |
| `length` | `i256` | Length of the init code in bytes. |

#### Result and purity

| Result | Purity |
|---|---|
| `i256` (created address; narrowable to `i160` on success, `0` on failure) | Effectful |

#### Annotations

None.

### `create2`

(`Statement::Create` with `CreateKind::Create2`)

#### Description

Deploy a new contract with a deterministic address derived from the caller's address, the salt, and the init-code hash. Same operand shape as [`create`](#create) plus an additional `salt`.

#### Syntax

```text
let $result := create2($value[: <type>], $offset[: <type>], $length[: <type>], $salt[: <type>])
```

#### Example

```text
let v5 := create2(v0, v1, v2, v3)
```

#### Operands

Same as [`create`](#create) plus `salt: i256`.

#### Result and purity

| Result | Purity |
|---|---|
| `i256` (created address; narrowable to `i160` on success, `0` on failure) | Effectful |

#### Annotations

None.

### `log<N>`

(`Statement::Log`)

#### Description

Emit an event log entry. The mnemonic suffix `<N>` is the number of indexed topics (`0` through `4`), determined by the length of the IR's `topics` field. The data portion is read from `length` bytes of emulated EVM linear memory at `offset`.

#### Syntax

```text
log<N>($offset[: <type>], $length[: <type>][, $topic_0[: <type>], …])
```

#### Example

```text
log0(v0, v1)
log2(v0, v1, v2, v3)            // two topics
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `offset` | `i256` | Data source offset in linear memory. |
| `length` | `i256` | Data length in bytes. |
| `topics` | `Vec<Value>` | Zero to four indexed topic values; the length determines the mnemonic suffix. |

#### Result and purity

| Result | Purity |
|---|---|
| None | Effectful |

#### Annotations

None.

## Termination

Statements that end the current call frame. Plain forms (`return`, `revert`, `stop`), unconditional traps (`invalid`, `selfdestruct`), and outlined revert variants (`panic_revert`, `error_string_revert`, `custom_error_revert`) that encode common Solidity error patterns into single nodes that can be deduplicated across call sites.

### `return`

(`Statement::Return`)

#### Description

End the current call frame successfully, returning `length` bytes from emulated EVM linear memory at `offset` as the return data.

#### Syntax

```text
return($offset[: <type>], $length[: <type>])
```

#### Example

```text
return(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `offset` | `i256` | Return-data source offset. |
| `length` | `i256` | Return-data length. |

#### Result and purity

| Result | Purity |
|---|---|
| None — terminates the call frame | Effectful (terminator) |

#### Annotations

None.

### `revert`

(`Statement::Revert`)

#### Description

End the current call frame with a revert, undoing all state changes made during the call, and returning `length` bytes of revert data from emulated EVM linear memory at `offset`.

#### Syntax

```text
revert($offset[: <type>], $length[: <type>])
```

#### Example

```text
revert(v0, v1)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `offset` | `i256` | Revert-data source offset. |
| `length` | `i256` | Revert-data length. |

#### Result and purity

| Result | Purity |
|---|---|
| None — terminates the call frame | Effectful (terminator) |

#### Annotations

None.

### `stop`

(`Statement::Stop`)

#### Description

End the current call frame successfully with empty return data.

#### Syntax

```text
stop()
```

#### Example

```text
stop()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| None — terminates the call frame | Effectful (terminator) |

#### Annotations

None.

### `invalid`

(`Statement::Invalid`)

#### Description

Unconditional invalid-opcode trap. Consumes all remaining gas and reverts. Used for unreachable branches and assertion failures.

#### Syntax

```text
invalid()
```

#### Example

```text
invalid()
```

#### Operands

None.

#### Result and purity

| Result | Purity |
|---|---|
| None — terminates the call frame | Effectful (terminator) |

#### Annotations

None.

### `selfdestruct`

(`Statement::SelfDestruct`)

#### Description

End the current call frame and transfer the contract's remaining balance to `address`. Post-Cancun, the contract storage is not deleted (selfdestruct is effectively deprecated; the opcode still exists for legacy compatibility).

#### Syntax

```text
selfdestruct($address[: <type>])
```

#### Example

```text
selfdestruct(v0: i160)
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `address` | `i256` | Recipient of the contract's balance; narrows to `i160`. |

#### Result and purity

| Result | Purity |
|---|---|
| None — terminates the call frame | Effectful (terminator) |

#### Annotations

None.

### `panic_revert`

(`Statement::PanicRevert`)

#### Description

Outlined Solidity panic revert. Equivalent to writing the `Panic(uint256)` ABI encoding (selector `0x4e487b71` plus the panic code) into emulated EVM linear memory and reverting, but emitted as a single statement that lowers to one outlined helper call. Common panic codes: `0x01` assertion failure, `0x11` arithmetic overflow, `0x12` division by zero, `0x32` array-out-of-bounds, `0x41` memory overflow.

#### Syntax

```text
panic_revert(0x<hex>)
```

#### Example

```text
panic_revert(0x11)              // arithmetic overflow
```

#### Operands

None — the panic code is stored as a `u8` field on the IR, not an SSA operand.

#### Result and purity

| Result | Purity |
|---|---|
| None — terminates the call frame | Effectful (terminator) |

#### Annotations

| Source field | Printed as |
|---|---|
| `code: u8` | The panic code in `0x<hex>` form (two hex digits, zero-padded). |

### `error_string_revert`

(`Statement::ErrorStringRevert`)

#### Description

Outlined Solidity `Error(string)` revert. Equivalent to writing the `Error` selector (`0x08c379a0`), the string offset and length, and up to four 32-byte data words into emulated EVM linear memory and reverting. The string length and the data words are stored as compile-time fields; no SSA operands.

#### Syntax

```text
error_string_revert(<length>, <N>_words)
```

#### Example

```text
error_string_revert(12, 1_words)        // 12-byte string in one 32-byte word
```

#### Operands

None — the string length and data are compile-time fields, not SSA operands.

#### Result and purity

| Result | Purity |
|---|---|
| None — terminates the call frame | Effectful (terminator) |

#### Annotations

| Source field | Printed as |
|---|---|
| `length: u8` | The string length in bytes, in the first syntax position. |
| `data: Vec<BigUint>` | The number of 32-byte data words (1–4), printed as `<N>_words` in the second syntax position. The actual data is stored separately and not shown in the printed form. |

### `custom_error_revert`

(`Statement::CustomErrorRevert`)

#### Description

Outlined Solidity custom-error revert. Encodes the error selector (left-shifted by 224 bits) and zero or more argument values into scratch memory and reverts. No FMP load is needed; the encoding uses the scratch region at offset `0`.

#### Syntax

```text
custom_error_revert(0x<hex>, [$arg_0, $arg_1, …])
```

#### Example

```text
custom_error_revert(0xa28c4c11, [v0, v1])
```

#### Operands

| Name | Type | Notes |
|---|---|---|
| `arguments` | `Vec<Value>` | Zero or more argument values; the selector is a compile-time field. |

#### Result and purity

| Result | Purity |
|---|---|
| None — terminates the call frame | Effectful (terminator) |

#### Annotations

| Source field | Printed as |
|---|---|
| `selector: BigUint` | The 4-byte error selector in hex, in the first syntax position. The selector is stored left-shifted by 224 bits; the printer right-shifts it back and prints the bare 4-byte value. |
