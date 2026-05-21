# newyork IR reference

A per-operation reference for the newyork IR: textual syntax, operand and result types, purity, region and static-slot annotations, and examples.

## How to read this reference

This appendix enumerates every operation the newyork IR supports. It is a lookup, not a walkthrough: each entry is self-contained and intended to be reachable by anchor.

Operations are grouped by function (memory and storage writes, pure expressions, control flow, and so on) rather than alphabetically. Jump to a specific operation from the [operation index](#operation-index) below, or use the sidebar.

Every operation appears in two places in the codebase. The canonical Rust definition is a variant of either `Expression` or `Statement` in `ir.rs`. The textual rendering used by debug dumps and by this appendix is produced by the printer in `printer.rs`. Treat the printed syntax as a debug surface, not a stable input language: there is no parser for it, and printer details change when passes add new annotations.

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

The umbrella enum. Three variants:

| Variant | Printed as | Description |
|---|---|---|
| `Int(BitWidth)` | `i1`, `i8`, …, `i256` | An integer at one of seven widths; see [BitWidth](#bitwidth). |
| `Ptr(AddressSpace)` | `ptr<heap>`, `ptr<stack>`, `ptr<storage>`, `ptr<code>` | A pointer tagged with its address space; see [AddressSpace](#addressspace). |
| `Void` | `void` | Unit type. Used for statements that produce no value and for `void`-returning functions. |

### `BitWidth`

The seven rungs of integer width. Newly minted values default to `I256`; type inference narrows them down to one of the lower rungs when it can prove the upper bits are zero or unused.

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
