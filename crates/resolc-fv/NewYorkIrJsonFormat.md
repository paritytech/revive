# The newyork IR JSON format

Reference for the artifacts `resolc --emit-ir-stages --debug-output-dir <dir>`
writes: every newyork IR primitive and the exact JSON it serializes to.

The format is whatever `serde_json` produces from the `Serialize` derives in
`crates/newyork/src/ir.rs`, so this document is a description of those types, not
a specification they are checked against. Every JSON fragment below was produced
by serializing the corresponding value; see [Provenance](#provenance).

Companion document: [`FormalVerification.md`](FormalVerification.md) covers why
the export exists and what consumes it. This one covers only the shape on disk.

---

## 1. What gets written

`--emit-ir-stages` requires `--debug-output-dir`. Into that directory, for a
contract whose dump stem is `<stem>`:

| File | Contents |
| --- | --- |
| `<stem>.stages.json` | The manifest — [`StageManifest`](#9-the-manifest) |
| `<stem>.<stage_id>.newyork.json` | One stage — [`StageRecord`](#8-the-stage-record) |
| `<stem>.yul.json` | The Yul AST the chain starts from |

Extensions come from `crates/common/src/extension.rs`
(`EXTENSION_NEWYORK_MANIFEST`, `EXTENSION_NEWYORK_JSON`, `EXTENSION_YUL_JSON`).
Stage identifiers are `<ordinal>.<pass>` or `<ordinal>.<scope>.<pass>`, e.g.
`007.fold_keccak`, `019.late0.simplify`. The ordinal is zero-padded to three
digits so file names sort lexically into pipeline order.

The human-readable pretty-printer (`crates/newyork/src/printer.rs`, the
`.newyork` dump) is a different, unrelated output. This document describes the
machine-readable `.newyork.json` only.

---

## 2. Encoding conventions

Five rules account for nearly all of the format.

**Enums are externally tagged.** This is serde's default. A variant carrying no
data is a bare string; a variant carrying data is a single-key object whose key
is the variant name:

```json
"Stop"
{"Revert": {"offset": …, "length": …}}
```

Struct variants nest an object of named fields; the one newtype variant
(`Expression::Var`) nests its payload directly.

**Newtype structs are transparent.** `ValueId(pub u32)` and
`FunctionId(pub u32)` serialize as bare numbers, not as `{"0": 7}`:

```json
7
```

**Numbers are `0x`-prefixed hex strings, with leading zeros dropped.** Every
`BigUint` field goes through `crate::serde_hex` rather than the derive, which
would emit a `u32` digit array. Zero is `"0x0"`.

```json
"value": "0xdeadbeef"
"value": "0x0"
```

**Byte strings are `0x`-prefixed, two digits per byte.** Unlike numbers they keep
leading and trailing zeros, so the length survives the round trip. The empty
byte string is `"0x"`. This applies to `Object::data` values only.

```json
"data": {"blob": "0x0001ff00", "empty": "0x"}
```

**Map keys become strings.** JSON requires it, so a `BTreeMap<FunctionId, _>`
keyed by a `u32` newtype emits decimal string keys:

```json
"functions": {"1": { … }}
```

Two further points:

- Fields typed `Option<T>` inside the IR are written as `null` when absent —
  they are *not* skipped. Only the four side tables on the stage record use
  `skip_serializing_if`.
- `u8` fields (`PanicRevert::code`, `ErrorStringRevert::length`) are plain JSON
  numbers in decimal, not hex strings. `PanicRevert { code: 0x11 }` is
  `{"PanicRevert": {"code": 17}}`.

### Notation used below

The `Value` object appears in almost every variant. To keep the tables readable
it is abbreviated as **`«V»`**, which always expands to:

```json
{"id": 3, "value_type": {"Int": "I256"}}
```

Likewise **`«R»`** abbreviates a [`Region`](#7-structural-containers) and
**`«E»`** an [`Expression`](#5-expressions).

---

## 3. Scalar primitives

| Type | Rust | JSON | Example |
| --- | --- | --- | --- |
| `BitWidth` | unit enum | string | `"I1"`, `"I8"`, `"I32"`, `"I64"`, `"I128"`, `"I160"`, `"I256"` |
| `AddressSpace` | unit enum | string | `"Heap"`, `"Stack"`, `"Storage"`, `"Code"` |
| `MemoryRegion` | unit enum | string | `"Scratch"`, `"FreePointerSlot"`, `"Dynamic"`, `"Unknown"` |
| `ValueId` | newtype `u32` | number | `3` |
| `FunctionId` | newtype `u32` | number | `3` |
| `CallKind` | unit enum | string | `"Call"`, `"CallCode"`, `"DelegateCall"`, `"StaticCall"` |
| `CreateKind` | unit enum | string | `"Create"`, `"Create2"` |
| `UnaryOperation` | unit enum | string | `"IsZero"`, `"Not"`, `"Clz"` |

`BitWidth` discriminants are the bit counts (`I160 = 160`), but serde emits the
*name*, not the number.

### `Type`

```json
{"Int": "I256"}
{"Ptr": "Heap"}
"Void"
```

`Int` carries a `BitWidth`, `Ptr` an `AddressSpace`, `Void` nothing. `Type` has a
`Default` of `Int(I256)`, but no field in the format is marked
`#[serde(default)]` or skipped, so a `value_type` is always written out.

### `Value`

A `ValueId` paired with its static `Type`:

```json
{"id": 3, "value_type": {"Int": "I256"}}
```

This is the **`«V»`** placeholder used throughout the tables below.

### `BinaryOperation`

A unit enum, serialized as its variant name. All 23 variants:

`"Add"`, `"Sub"`, `"Mul"`, `"Div"`, `"SDiv"`, `"Mod"`, `"SMod"`, `"Exp"`,
`"AddMod"`, `"MulMod"`, `"And"`, `"Or"`, `"Xor"`, `"Shl"`, `"Shr"`, `"Sar"`,
`"Lt"`, `"Gt"`, `"Slt"`, `"Sgt"`, `"Eq"`, `"Byte"`, `"SignExtend"`.

`AddMod` and `MulMod` appear only under `Expression::Ternary`; the rest appear
under `Expression::Binary`.

---

## 4. Shared field names

The same field names recur with consistent meaning:

| Field | Meaning |
| --- | --- |
| `offset`, `length` | A byte range `[offset, offset + length)` in the addressed space |
| `destination`, `source` | Copy endpoints |
| `key`, `value` | Storage/transient slot key and the word written or read |
| `static_slot` | The constant folding of `key`, or `null` when it is not statically known |
| `region` | [`MemoryRegion`](#3-scalar-primitives) annotation on a heap access |
| `result` | A bare `ValueId` — the SSA value a side-effecting statement defines |
| `bindings`, `outputs`, `loop_variables` | Arrays of bare `ValueId`, the values being defined |
| `inputs`, `yields`, `values` | Arrays of `«V»`, the values being consumed |

Note the asymmetry: a position that *defines* an SSA name carries a bare
`ValueId` (a number), while a position that *uses* one carries a full `Value`
(an object with a type). `Let` binds `[1, 2]` but reads `{"id": 0, …}`.

---

## 5. Expressions

Pure, value-producing operations. All 45 variants.

### Nullary — environment queries

Each serializes as a bare string, with no payload:

`"CallValue"`, `"Caller"`, `"Origin"`, `"CallDataSize"`, `"CodeSize"`,
`"GasPrice"`, `"ReturnDataSize"`, `"Coinbase"`, `"Timestamp"`, `"Number"`,
`"Difficulty"`, `"GasLimit"`, `"ChainId"`, `"SelfBalance"`, `"BaseFee"`,
`"BlobBaseFee"`, `"Gas"`, `"MSize"`, `"Address"`.

### The rest

| Variant | JSON |
| --- | --- |
| `Literal` | `{"Literal":{"value":"0xff","value_type":{"Int":"I256"}}}` |
| `Var` | `{"Var":7}` |
| `Binary` | `{"Binary":{"operation":"Add","lhs":«V»,"rhs":«V»}}` |
| `Ternary` | `{"Ternary":{"operation":"AddMod","a":«V»,"b":«V»,"n":«V»}}` |
| `Unary` | `{"Unary":{"operation":"IsZero","operand":«V»}}` |
| `CallDataLoad` | `{"CallDataLoad":{"offset":«V»}}` |
| `ExtCodeSize` | `{"ExtCodeSize":{"address":«V»}}` |
| `ExtCodeHash` | `{"ExtCodeHash":{"address":«V»}}` |
| `Balance` | `{"Balance":{"address":«V»}}` |
| `BlockHash` | `{"BlockHash":{"number":«V»}}` |
| `BlobHash` | `{"BlobHash":{"index":«V»}}` |
| `MLoad` | `{"MLoad":{"offset":«V»,"region":"Dynamic"}}` |
| `SLoad` | `{"SLoad":{"key":«V»,"static_slot":"0x3"}}` |
| `SLoad` (dynamic key) | `{"SLoad":{"key":«V»,"static_slot":null}}` |
| `TLoad` | `{"TLoad":{"key":«V»}}` |
| `Call` | `{"Call":{"function":2,"arguments":[«V»,«V»]}}` |
| `Truncate` | `{"Truncate":{"value":«V»,"to":"I64"}}` |
| `ZeroExtend` | `{"ZeroExtend":{"value":«V»,"to":"I256"}}` |
| `SignExtendTo` | `{"SignExtendTo":{"value":«V»,"to":"I256"}}` |
| `Keccak256` | `{"Keccak256":{"offset":«V»,"length":«V»}}` |
| `Keccak256Pair` | `{"Keccak256Pair":{"word0":«V»,"word1":«V»}}` |
| `Keccak256Single` | `{"Keccak256Single":{"word0":«V»}}` |
| `MappingSLoad` | `{"MappingSLoad":{"key":«V»,"slot":«V»}}` |
| `DataOffset` | `{"DataOffset":{"id":"Test_deployed"}}` |
| `DataSize` | `{"DataSize":{"id":"Test_deployed"}}` |
| `LoadImmutable` | `{"LoadImmutable":{"key":"7"}}` |
| `LinkerSymbol` | `{"LinkerSymbol":{"path":"contracts/Library.sol:L"}}` |

Notes:

- `Literal.value` is hex; `Literal.value_type` is a full `Type`, so a literal
  carries its own width independently of the values around it.
- `Var` is the only newtype variant — its payload is a bare `ValueId`, so the
  value is `{"Var": 7}` rather than `{"Var": {"id": 7}}`.
- `Call.function` is a bare `FunctionId`, resolved against the **enclosing
  object's** `functions` map. IDs are not unique across objects; a subobject
  reusing `FunctionId(0)` refers to a different function.
- `DataOffset.id` / `DataSize.id` name a key of some object's `data` map or a
  subobject; `LoadImmutable.key` and `SetImmutable.key` are the Solidity
  immutable identifier as a string.
- `Keccak256Pair`, `Keccak256Single`, `MappingSLoad` are outlining results: they
  do not come out of `from_yul`, they appear once `fold_keccak` /
  `outline_mapping` have run.

---

## 6. Statements

Effectful operations and structured control flow. All 32 variants.

### Unit variants

`"Stop"` and `"Invalid"` — bare strings, no payload.

### Bindings and memory

| Variant | JSON |
| --- | --- |
| `Let` | `{"Let":{"bindings":[1,2],"value":«E»}}` |
| `MStore` | `{"MStore":{"offset":«V»,"value":«V»,"region":"Scratch"}}` |
| `MStore8` | `{"MStore8":{"offset":«V»,"value":«V»,"region":"Unknown"}}` |
| `MCopy` | `{"MCopy":{"destination":«V»,"source":«V»,"length":«V»}}` |
| `SStore` | `{"SStore":{"key":«V»,"value":«V»,"static_slot":"0x0"}}` |
| `TStore` | `{"TStore":{"key":«V»,"value":«V»}}` |
| `MappingSStore` | `{"MappingSStore":{"key":«V»,"slot":«V»,"value":«V»}}` |

`Let.bindings` is an array because an expression may produce several values (a
multi-return `Call`); `Let.value` is a nested `Expression`.

### Structured control flow

| Variant | JSON |
| --- | --- |
| `If` | `{"If":{"condition":«V»,"inputs":[«V»],"then_region":«R»,"else_region":«R»,"outputs":[10]}}` |
| `Switch` | `{"Switch":{"scrutinee":«V»,"inputs":[«V»],"cases":[{"value":"0x1","body":«R»}],"default":«R»,"outputs":[10]}}` |
| `For` | `{"For":{"initial_values":[«V»],"loop_variables":[2],"condition_statements":[],"condition":«E»,"body":«R»,"post_input_variables":[3],"post":«R»,"outputs":[4]}}` |
| `Break` | `{"Break":{"values":[«V»]}}` |
| `Continue` | `{"Continue":{"values":[«V»]}}` |
| `Leave` | `{"Leave":{"return_values":[«V»]}}` |
| `Block` | `{"Block":«R»}` |

These are the SSA-carrying constructs, and the field split matters:

- `inputs` (uses, `«V»`) are the values flowing *into* the regions;
  `outputs` (definitions, bare `ValueId`) are the names the construct binds
  afterwards. A region supplies them through its `yields`.
- `If.else_region` is `Option<Region>` and serializes as `null` when absent — an
  absent else region means "yield the inputs unchanged". `Switch.default` is the
  same.
- `For` is the most involved: `loop_variables` are the names visible inside the
  loop, `condition_statements` run before each condition evaluation,
  `post_input_variables` receive the body's yields, and `outputs` are the names
  bound after the loop exits. `Break` and `Continue` carry the loop-carried
  values at their site, which is why they have a `values` array.
- `Statement::Block` wraps a **`Region`**, not the `Block` struct. Its JSON is
  therefore `{"statements": […], "yields": […]}` — with a `yields` key — while
  the `Block` struct used for function bodies has only `statements`.

### Termination and reverts

| Variant | JSON |
| --- | --- |
| `Revert` | `{"Revert":{"offset":«V»,"length":«V»}}` |
| `Return` | `{"Return":{"offset":«V»,"length":«V»}}` |
| `SelfDestruct` | `{"SelfDestruct":{"address":«V»}}` |
| `PanicRevert` | `{"PanicRevert":{"code":17}}` |
| `ErrorStringRevert` | `{"ErrorStringRevert":{"length":5,"data":["0x1000…000"]}}` |
| `CustomErrorRevert` | `{"CustomErrorRevert":{"selector":"0x100000000000000000000000000000000000000000000000000000000","arguments":[«V»]}}` |

- `PanicRevert.code` is a decimal `u8` — `17` is Solidity's `0x11` overflow
  panic.
- `ErrorStringRevert.length` is a decimal `u8` byte count; `data` is an array of
  hex strings, one per 32-byte word (1–4 of them).
- `CustomErrorRevert.selector` is the 4-byte selector already left-shifted by
  224 bits, as it sits in scratch memory — hence the long hex string.
  `arguments` holds 0–3 values.

### Calls, creation, logging, copies

| Variant | JSON |
| --- | --- |
| `ExternalCall` | `{"ExternalCall":{"kind":"Call","gas":«V»,"address":«V»,"value":«V»,"args_offset":«V»,"args_length":«V»,"ret_offset":«V»,"ret_length":«V»,"result":8}}` |
| `ExternalCall` (no value) | `…,"kind":"DelegateCall",…,"value":null,…` |
| `Create` | `{"Create":{"kind":"Create2","value":«V»,"offset":«V»,"length":«V»,"salt":«V»,"result":5}}` |
| `Log` | `{"Log":{"offset":«V»,"length":«V»,"topics":[«V»,«V»]}}` |
| `CodeCopy` | `{"CodeCopy":{"destination":«V»,"offset":«V»,"length":«V»}}` |
| `ExtCodeCopy` | `{"ExtCodeCopy":{"address":«V»,"destination":«V»,"offset":«V»,"length":«V»}}` |
| `ReturnDataCopy` | `{"ReturnDataCopy":{"destination":«V»,"offset":«V»,"length":«V»}}` |
| `DataCopy` | `{"DataCopy":{"destination":«V»,"offset":«V»,"length":«V»}}` |
| `CallDataCopy` | `{"CallDataCopy":{"destination":«V»,"offset":«V»,"length":«V»}}` |

- All four call kinds share one variant, discriminated by `kind`. `value` is
  `null` for `DelegateCall` and `StaticCall`, which cannot transfer value.
- `Create.salt` is `null` for `kind: "Create"` and present for `"Create2"`.
- `result` is a bare `ValueId`: the success flag for `ExternalCall`, the deployed
  address for `Create`.
- `Log` has no arity variants — `log0`…`log4` all map here, distinguished by the
  length of `topics`.

### Remaining

| Variant | JSON |
| --- | --- |
| `Expression` | `{"Expression":"Gas"}` |
| `SetImmutable` | `{"SetImmutable":{"key":"7","value":«V»}}` |

`Statement::Expression` wraps an expression evaluated for its side effects with
the result discarded, so its payload is a nested `Expression` in whatever form
that variant takes.

---

## 7. Structural containers

### `Region`

A statement sequence that yields values to its enclosing construct:

```json
{"statements": [], "yields": [{"id": 9, "value_type": {"Int": "I256"}}]}
```

### `Block`

A statement sequence that does not yield — used for function bodies and object
top-level code. Distinguished from `Region` by the *absence* of `yields`:

```json
{"statements": ["Stop"]}
```

### `SwitchCase`

```json
{"value": "0xff", "body": «R»}
```

`value` is the hex-encoded selector constant that triggers the case.

### `Function`

```json
{
  "id": 1,
  "name": "fun_transfer",
  "parameters": [[0, {"Int": "I256"}]],
  "returns": [{"Int": "I64"}],
  "return_values_initial": [1],
  "return_values": [2],
  "body": {"statements": [ … ]},
  "call_count": 3,
  "size_estimate": 12
}
```

- `parameters` is an array of two-element arrays — a Rust `(ValueId, Type)` tuple
  becomes a JSON array, *not* an object.
- `returns` holds types only; the SSA names are in the two `return_values*`
  arrays. `return_values_initial` are the entry-point names (what an `If` in the
  body references as the "before" value), `return_values` the post-body names
  stored to the return pointer.
- `call_count` and `size_estimate` are inlining heuristics, refreshed by the
  `estimate_sizes` pass; they are advisory and carry no semantics.

### `Object`

The top-level container, and the value under the stage record's `ir` key:

```json
{
  "name": "Test",
  "code": {"statements": ["Stop"]},
  "functions": {"1": { … }},
  "subobjects": [
    {"name": "Test_deployed", "code": {"statements": []},
     "functions": {}, "subobjects": [], "data": {}}
  ],
  "data": {"blob": "0x0001ff00", "empty": "0x"}
}
```

- `functions` keys are decimal `FunctionId` strings, and each entry repeats its
  own `id` inside the value.
- `subobjects` is the recursion: a contract's deploy object holds its runtime
  object here. Each level has its own independent `FunctionId` namespace.
- `data` values are byte strings, two hex digits per byte — the only place in
  the format where that encoding is used.

---

## 8. The stage record

One `.newyork.json` file holds one `StageRecord`: the IR after a pass plus the
side tables codegen reads. The four side tables are the only keys in the whole
format with `skip_serializing_if`, so a key's **absence** means that capture site
had no such table to hand — not that the table was empty.

```json
{
  "stage_id": "019.late0.simplify",
  "pass": "simplify",
  "object": "Test",
  "ir": { … an Object … },
  "type_info": { … },
  "heap_opt": { … },
  "inline_results": { … },
  "mem_opt": { … }
}
```

| Key | Type | Always present |
| --- | --- | --- |
| `stage_id` | string | yes |
| `pass` | string | yes |
| `object` | string | yes |
| `ir` | [`Object`](#object) | yes |
| `type_info` | `ResolvedTypes` | at `narrow`, `late<N>.narrow`, `heap_opt`, `final` |
| `heap_opt` | `HeapOptResults` | at `heap_opt`, `final` |
| `inline_results` | `InlineResults` | at `final` |
| `mem_opt` | `MemOptResults` | at `final` |

Every other stage carries none of the four. In particular the intermediate
late-loop stages (`late<N>.inline`, `late<N>.simplify`, …) drop `type_info`
again even though a `narrow` preceded them.

`object` is the Yul object name, which is what separates a contract's deploy
stages from its runtime stages — both run the same pass sequence.

### `type_info` — `ResolvedTypes`

```json
{
  "values": {
    "0": {
      "constraint": {"min_width": "I64", "max_width": "I256", "is_signed": false},
      "use_demand_width": "I256"
    }
  },
  "fmp_could_be_unbounded": false
}
```

`values` is keyed by decimal SSA value id. The width codegen emits is
`constraint.min_width`; `use_demand_width` is the widest width any use site
needs, computed from recorded use contexts rather than from the eagerly narrowed
`max_width`, so it is not derivable from `constraint`.

### `heap_opt` — `HeapOptResults`

```json
{
  "native_safe_regions": [],
  "native_safe_offsets": [],
  "total_accesses": 0,
  "unknown_accesses": 0,
  "tainted_count": 0,
  "escaping_count": 0,
  "has_dynamic_escapes": false,
  "min_dynamic_escape_start": null,
  "has_dynamic_accesses": false,
  "fmp_word_escapes": false,
  "variable_accessed_offsets": [],
  "fmp_could_be_unbounded": false,
  "fmp_slot_tainted": false
}
```

The two `native_safe_*` sets and `variable_accessed_offsets` are `BTreeSet<u64>`,
serialized as sorted arrays of decimal numbers. This is the analysis that decides
where byte-swapping can be skipped. Note that several fields are private in Rust
but still serialized — serde derives over all fields regardless of visibility, so
the JSON is wider than the crate's public API.

### `inline_results` — `InlineResults`

```json
{
  "inlined_call_sites": 4,
  "removed_functions": [2],
  "decisions": {"1": "AlwaysInline"}
}
```

`decisions` is keyed by decimal `FunctionId`; values are `"AlwaysInline"`,
`"NeverInline"` or `"CostBenefit"`.

### `mem_opt` — `MemOptResults`

Six counters, all decimal numbers:

```json
{
  "loads_eliminated": 0,
  "stores_eliminated": 0,
  "values_tracked": 0,
  "keccak_pairs_fused": 0,
  "keccak_singles_fused": 0,
  "fmp_loads_eliminated": 0
}
```

---

## 9. The manifest

`<stem>.stages.json` indexes the stage files in pipeline order:

```json
{
  "schema": 1,
  "contract": "contracts/Test.sol:Test",
  "heap_size": 65536,
  "yul_ast": "Test.yul.json",
  "stages": [
    {
      "id": "000.from_yul",
      "pass": "from_yul",
      "object": "Test",
      "ir_hash": "0x…",
      "file": "Test.000.from_yul.newyork.json"
    }
  ]
}
```

| Key | Meaning |
| --- | --- |
| `schema` | `STAGE_MANIFEST_SCHEMA`, currently `1`. Raised whenever the shape changes; reject an unknown version rather than guessing at fields |
| `contract` | The Yul object identifier the chain was compiled from |
| `heap_size` | The configured EVM heap size in bytes, which the heap analysis depends on |
| `yul_ast` | **File name** of the dumped Yul AST beside the manifest, not the AST itself. `null` when it was not written |
| `stages` | Every stage, in pipeline order |

`ManifestStage.ir_hash` is a Keccak-256 hash over the serialized `ir` **alone**,
excluding the side tables and the metadata. Equal hashes across adjacent stages
of one object mean the pass left the IR untouched — though it may still have
moved a side table.

Key on `pass`, not on `id`: the ordinal shifts whenever the pipeline gains or
loses a pass, while the pass name is stable.

The codegen dumps (LLVM IR, assembly, the object file) are written after the
manifest and share its file-name stem, so they are not listed in `stages`.

---

## 10. The stage sequence

`translate_yul_object` captures, in order:

1. `from_yul`
2. the optimization pipeline, run for the top-level object and then recursively
   for each subobject: `inline`, `simplify`, `dedup`, `dedup_fuzzy`, `mem_opt`,
   `fmp_prop`, `fold_keccak`, `simplify`, `outline_mapping`, `guard_narrow`,
   `simplify`, `const_params`, `shrink_predict`, `dedup`, `dedup_fuzzy`
3. `narrow` — first stage carrying `type_info`
4. the late-inline loop, each iteration scoped `late<N>`: `estimate_sizes`,
   `inline`, `simplify`, `outline_mapping`, `guard_narrow`, `simplify`, `dedup`,
   `dedup_fuzzy`, `narrow`
5. `estimate_sizes`
6. `heap_opt` — first stage carrying `heap_opt`
7. `final` — carries all four side tables

Two simplify passes, after `const_params` and after `shrink_predict`, are
guarded and emit a stage only when they fire. So the sequence is not fixed in
length; read it from the manifest rather than assuming it.

### What `ir` covers at each stage

Step 2 is the only part that runs per object, and this changes what a record's
`ir` holds:

- A **step-2 stage** is captured on one object at a time. For a subobject, the
  record's `object` is the subobject's name and its `ir` is that subobject alone
  — its `subobjects` array is whatever that object nests, not the parent. A
  contract therefore produces two runs of the step-2 sequence, one per half,
  distinguished only by `object`.
- Every **other stage** (`from_yul`, `narrow`, the `late<N>` scope,
  `estimate_sizes`, `heap_opt`, `final`) is captured once on the root object, so
  its `ir` is the whole tree with the subobjects nested inside, and its `object`
  is the root's name.

A consumer pairing adjacent stages must account for this: a subobject's
step-2 `ir` is a *subtree* of the root `ir` in the `narrow` record that follows,
not a successor state of the same value. Group by `object` before pairing.

`final` is the IR codegen actually received. It is frequently byte-identical to
`heap_opt` — whenever the free memory pointer is bounded — but its side tables
differ, because `heap_opt` runs after the narrowing fixed point and can force a
full re-inference.

---

## Provenance

Every JSON fragment in this document was produced by serializing the
corresponding Rust value with `serde_json`, using the type definitions as they
stand in:

| Source | Defines |
| --- | --- |
| `crates/newyork/src/ir.rs` | Every IR primitive in sections 3–7 |
| `crates/newyork/src/serde_hex.rs` | The hex encodings in section 2 |
| `crates/newyork/src/export.rs` | `StageRecord`, `StageManifest`, `ManifestStage`, stage identifiers |
| `crates/newyork/src/type_inference.rs` | `ResolvedTypes`, `ResolvedType`, `TypeConstraint` |
| `crates/newyork/src/heap_opt.rs` | `HeapOptResults` |
| `crates/newyork/src/inline.rs` | `InlineResults`, `InlineDecision` |
| `crates/newyork/src/mem_opt.rs` | `MemOptResults` |
| `crates/newyork/src/lib.rs` | The stage sequence in section 10 |
| `crates/common/src/extension.rs` | The file extensions in section 1 |
| `crates/llvm-context/src/debug_config/mod.rs` | The file naming in section 1 |

The format has no independent definition — it is whatever those derives emit.
When a field is added or a `#[serde(...)]` attribute changes, this document goes
stale silently; `crates/newyork/src/serde_hex.rs` carries round-trip tests that
pin the hex encodings, but nothing pins the rest.
