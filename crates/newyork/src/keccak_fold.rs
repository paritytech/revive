//! Constant folding of fused keccak256 expressions, guarded by scratch liveness.
//!
//! The `Keccak256Pair`/`Keccak256Single` helpers write their hash inputs back to
//! scratch `[0, 0x40)`/`[0, 0x20)` (see `Keccak256OneWord::emit_body` /
//! `Keccak256TwoWords::emit_body`), and `mem_opt`'s fusion dead-eliminates the
//! original staging `mstore`s on the strength of that write-back. Folding a fused
//! node with constant operands to a literal removes the helper call — and with it
//! the write-back — so a later read of scratch across a load-forwarding boundary
//! would observe unwritten memory instead of the hashed inputs.
//!
//! This pass folds every constant-operand fused keccak, but first computes which
//! scratch words may still be read after each fold site: a backward may-liveness
//! analysis over the two scratch words, interprocedural across the object's
//! functions (per-function gen/kill summaries plus caller-aware exit liveness).
//! Words that are provably dead fold to a bare literal — the common case, since
//! solc-generated code never re-reads scratch as data after a keccak. Words that
//! may still be read get an explicit write-back `mstore` of the constant input
//! right after the folded binding, preserving the EVM-visible heap state at
//! exactly the sites that could observe it.
//!
//! Dynamic offsets would make that analysis useless — solc reads memory almost
//! exclusively through free-memory-pointer ranges — so the pass also computes
//! *pointer provenance*: values derived from `mload(0x40)` (or constants
//! `>= 0x60`) through `add`, trusted `and` masks, copies, branch merges,
//! loop-carried variables, parameters, and returns are classified as trusted
//! pointers that can never reach scratch. That classification is only sound
//! while the FMP slot `[0x40, 0x60)` provably holds a trusted pointer, which
//! `collect_facts` verifies for every write it can attribute; the remaining
//! vector (solc's benign misaligned revert-string stores over the slot) is
//! ruled out via the heap analysis' observed-corruption scan. Reads at offsets
//! with no provenance are assumed to reach scratch — such sites pay one or two
//! extra stores; they never miscompile.
//!
//! **Known gaps (deliberate).** Pointer provenance treats `add(pointer, x)` as
//! a pointer for arbitrary `x` and accepts `guard_narrow`'s all-ones width
//! masks as identity. A 256-bit `add` that wraps past 2^256, or an FMP first
//! grown beyond the mask width and then truncated, could produce a scratch
//! offset that this classification misses, and a program reading `mload(0x40)`
//! before ever initializing the slot observes scratch at offset zero. All of
//! these require hand-written Yul engineered around the Solidity memory model
//! (solc guards its allocator against exactly these overflows); they are the
//! same class of deliberately accepted gap as the dynamic-store wrap gap
//! documented on `fmp_could_be_unbounded` in `heap_opt`, and closing them
//! would forfeit most of the fold in exchange.

use std::collections::{BTreeMap, BTreeSet};

use num::{BigUint, ToPrimitive};

use revive_common::BYTE_LENGTH_WORD;

use crate::ir::{
    BitWidth, Expression, FunctionId, MemoryRegion, Object, Statement, Type, Value, ValueId,
};

/// Start offset of the second scratch word `[0x20, 0x40)`.
const SCRATCH_WORD1_OFFSET: u64 = BYTE_LENGTH_WORD as u64;

/// First offset past the scratch region `[0, 0x40)`.
const SCRATCH_END: u64 = 2 * BYTE_LENGTH_WORD as u64;

/// Converts a BigUint into a 32-byte big-endian buffer.
fn biguint_to_be32(value: &BigUint) -> [u8; 32] {
    let mut buffer = [0u8; 32];
    let bytes = value.to_bytes_be();
    let start = 32usize.saturating_sub(bytes.len());
    buffer[start..].copy_from_slice(&bytes[bytes.len().saturating_sub(32)..]);
    buffer
}

/// Computes keccak256 of a single 256-bit word at compile time.
pub(crate) fn fold_keccak256_single(word0: &BigUint) -> BigUint {
    let buffer = biguint_to_be32(word0);
    let hash = revive_common::Keccak256::from_slice(&buffer);
    BigUint::from_bytes_be(hash.as_bytes())
}

/// Computes keccak256 of two 256-bit words at compile time.
pub(crate) fn fold_keccak256_pair(word0: &BigUint, word1: &BigUint) -> BigUint {
    let mut buffer = [0u8; 64];
    buffer[..32].copy_from_slice(&biguint_to_be32(word0));
    buffer[32..].copy_from_slice(&biguint_to_be32(word1));
    let hash = revive_common::Keccak256::from_slice(&buffer);
    BigUint::from_bytes_be(hash.as_bytes())
}

/// Backward may-liveness of the two scratch words a fused keccak helper writes back.
///
/// A word is *live* at a program point when some path from that point may read
/// any of its bytes before every one of them is overwritten.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct ScratchLiveness {
    /// Scratch word `[0x00, 0x20)`.
    word0: bool,
    /// Scratch word `[0x20, 0x40)`.
    word1: bool,
}

impl ScratchLiveness {
    const DEAD: ScratchLiveness = ScratchLiveness {
        word0: false,
        word1: false,
    };

    const LIVE: ScratchLiveness = ScratchLiveness {
        word0: true,
        word1: true,
    };

    /// Path merge: live on either path stays live.
    fn join(self, other: ScratchLiveness) -> ScratchLiveness {
        ScratchLiveness {
            word0: self.word0 || other.word0,
            word1: self.word1 || other.word1,
        }
    }

    /// Removes the words fully overwritten by a must-write.
    fn minus(self, kills: ScratchLiveness) -> ScratchLiveness {
        ScratchLiveness {
            word0: self.word0 && !kills.word0,
            word1: self.word1 && !kills.word1,
        }
    }
}

/// Scratch-liveness transfer of one function, as observed across a call to it.
///
/// The per-word transfer is `before(w) = entry_if_exit_live(w)` when `w` is live
/// after the call returns, and `entry_if_exit_dead(w)` when it is dead. The two
/// components are the results of analyzing the body against an all-dead and an
/// all-live exit state; per-word independence of the transfer makes two runs
/// sufficient.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct FunctionSummary {
    /// Words the function itself may read before overwriting them.
    entry_if_exit_dead: ScratchLiveness,
    /// Additionally keeps words live that the function does not reliably
    /// overwrite on every returning path.
    entry_if_exit_live: ScratchLiveness,
}

impl FunctionSummary {
    /// Applies the summarized transfer to the liveness after the call.
    fn apply(self, after: ScratchLiveness) -> ScratchLiveness {
        ScratchLiveness {
            word0: if after.word0 {
                self.entry_if_exit_live.word0
            } else {
                self.entry_if_exit_dead.word0
            },
            word1: if after.word1 {
                self.entry_if_exit_live.word1
            } else {
                self.entry_if_exit_dead.word1
            },
        }
    }
}

/// Compile-time classification of a memory offset or length operand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StaticExtent {
    /// Constant that fits `u64`.
    Known(u64),
    /// Constant too large for `u64`; as an offset it lies past scratch, and any
    /// access through it traps or exhausts gas before observing memory.
    Huge,
    /// Derived from the free memory pointer under proven FMP-slot integrity:
    /// at least `0x60` at runtime, so as an offset it never reaches scratch.
    /// See the module documentation for the deliberate wrap gap.
    FreePointerDerived,
    /// Not a compile-time constant.
    Dynamic,
}

/// Continuation states for `break`/`continue` of the innermost enclosing loop.
#[derive(Clone, Copy)]
struct LoopContext {
    /// Liveness after the enclosing `For` statement.
    break_liveness: ScratchLiveness,
    /// Liveness at the start of the enclosing loop's post region.
    continue_liveness: ScratchLiveness,
}

/// Mutable state of the final rewrite phase.
struct RewriteState {
    /// Source of fresh SSA ids for the write-back statements.
    next_value_id: ValueId,
    /// Number of fused keccak expressions folded to literals.
    folded: usize,
    /// Number of write-back stores emitted for live scratch words.
    write_back_stores: usize,
}

/// One backward walk over a statement tree.
///
/// The same walker implements all three phases: function summary computation
/// (both continuations `None`), exit-liveness propagation (`call_site_liveness`
/// set), and the final fold/rewrite (`rewrite` set).
struct ScratchLivenessWalker<'pass> {
    /// Object-wide constants, including hashes of fold candidates.
    constants: &'pass BTreeMap<u32, BigUint>,
    /// Values provably `>= 0x60` because they derive from the free memory
    /// pointer; empty when FMP-slot integrity could not be established.
    free_pointer_derived: &'pass BTreeSet<u32>,
    /// Current transfer summaries of the object's functions.
    summaries: &'pass BTreeMap<FunctionId, FunctionSummary>,
    /// Liveness assumed at `Leave` and at body fall-through of this container.
    exit_liveness: ScratchLiveness,
    /// When set, accumulates the liveness observed after each internal call.
    call_site_liveness: Option<&'pass mut BTreeMap<FunctionId, ScratchLiveness>>,
    /// When set, candidates are folded in place and write-backs inserted.
    rewrite: Option<&'pass mut RewriteState>,
}

impl ScratchLivenessWalker<'_> {
    /// Processes a statement list backward, returning the liveness at its start.
    fn process_statements(
        &mut self,
        statements: &mut Vec<Statement>,
        after: ScratchLiveness,
        loop_context: Option<LoopContext>,
    ) -> ScratchLiveness {
        let mut state = after;
        let mut index = statements.len();
        while index > 0 {
            index -= 1;
            if let Some(write_back) = self.try_fold_candidate(&mut statements[index], &mut state) {
                // Insertions land behind the backward cursor and are not revisited.
                statements.splice(index + 1..index + 1, write_back);
                continue;
            }
            state = self.transfer_statement(&mut statements[index], state, loop_context);
        }
        state
    }

    /// Intercepts constant-operand fused keccak bindings.
    ///
    /// Returns `None` when the statement is not a fold candidate. In analysis
    /// phases candidates transfer as identity — the eventual fold decides
    /// per-site whether the write-back survives, so assuming neither the
    /// helper's write-back nor a scratch read is the conservative middle.
    /// In the rewrite phase the binding is replaced by the literal hash and the
    /// returned statements write the constant inputs back for the words that
    /// are live; `state` is updated to the liveness before the folded site.
    fn try_fold_candidate(
        &mut self,
        statement: &mut Statement,
        state: &mut ScratchLiveness,
    ) -> Option<Vec<Statement>> {
        let Statement::Let { bindings, value } = statement else {
            return None;
        };
        if bindings.len() != 1 {
            return None;
        }
        let (hash, inputs) = match value {
            Expression::Keccak256Single { word0 } => {
                let word0 = self.constants.get(&word0.id.0)?;
                (fold_keccak256_single(word0), vec![(0u64, word0.clone())])
            }
            Expression::Keccak256Pair { word0, word1 } => {
                let word0 = self.constants.get(&word0.id.0)?;
                let word1 = self.constants.get(&word1.id.0)?;
                (
                    fold_keccak256_pair(word0, word1),
                    vec![(0u64, word0.clone()), (SCRATCH_WORD1_OFFSET, word1.clone())],
                )
            }
            _ => return None,
        };

        let Some(rewrite) = self.rewrite.as_deref_mut() else {
            return Some(Vec::new());
        };

        *value = Expression::Literal {
            value: hash,
            value_type: Type::Int(BitWidth::I256),
        };
        rewrite.folded += 1;

        let mut write_back = Vec::new();
        let mut kills = ScratchLiveness::DEAD;
        for (offset, input) in inputs {
            let word_is_live = match offset {
                0 => state.word0,
                _ => state.word1,
            };
            if !word_is_live {
                continue;
            }
            let offset_id = rewrite.next_value_id.fresh();
            let input_id = rewrite.next_value_id.fresh();
            write_back.push(Statement::Let {
                bindings: vec![offset_id],
                value: Expression::Literal {
                    value: BigUint::from(offset),
                    value_type: Type::Int(BitWidth::I256),
                },
            });
            write_back.push(Statement::Let {
                bindings: vec![input_id],
                value: Expression::Literal {
                    value: input,
                    value_type: Type::Int(BitWidth::I256),
                },
            });
            write_back.push(Statement::MStore {
                offset: Value::int(offset_id),
                value: Value::int(input_id),
                region: MemoryRegion::Scratch,
            });
            rewrite.write_back_stores += 1;
            match offset {
                0 => kills.word0 = true,
                _ => kills.word1 = true,
            }
        }
        *state = state.minus(kills);
        Some(write_back)
    }

    /// Backward transfer of a single non-candidate statement.
    fn transfer_statement(
        &mut self,
        statement: &mut Statement,
        after: ScratchLiveness,
        loop_context: Option<LoopContext>,
    ) -> ScratchLiveness {
        match statement {
            Statement::Let { value, .. } => self.transfer_expression(value, after),
            Statement::Expression(expression) => self.transfer_expression(expression, after),

            Statement::MStore { offset, .. } => {
                let kills = Self::range_write_kills(
                    self.static_extent(offset),
                    StaticExtent::Known(BYTE_LENGTH_WORD as u64),
                );
                after.minus(kills)
            }
            // A single-byte store never fully overwrites a scratch word.
            Statement::MStore8 { .. } => after,
            Statement::MCopy {
                destination,
                source,
                length,
            } => {
                let kills = Self::range_write_kills(
                    self.static_extent(destination),
                    self.static_extent(length),
                );
                let reads = Self::range_read_liveness(
                    self.static_extent(source),
                    self.static_extent(length),
                );
                after.minus(kills).join(reads)
            }

            Statement::SStore { .. }
            | Statement::TStore { .. }
            | Statement::SetImmutable { .. } => after,

            // The outlined compound helper stages its keccak input in a
            // function-local buffer, touching no scratch; identity is also the
            // conservative choice should its lowering ever write scratch back.
            Statement::MappingSStore { .. } => after,

            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                let then_liveness =
                    self.process_statements(&mut then_region.statements, after, loop_context);
                let else_liveness = match else_region {
                    Some(region) => {
                        self.process_statements(&mut region.statements, after, loop_context)
                    }
                    None => after,
                };
                then_liveness.join(else_liveness)
            }
            Statement::Switch { cases, default, .. } => {
                // Without a default region, no-match falls through to `after`.
                let mut merged = match default {
                    Some(region) => {
                        self.process_statements(&mut region.statements, after, loop_context)
                    }
                    None => after,
                };
                for case in cases.iter_mut() {
                    merged = merged.join(self.process_statements(
                        &mut case.body.statements,
                        after,
                        loop_context,
                    ));
                }
                merged
            }
            Statement::For {
                condition_statements,
                condition,
                body,
                post,
                ..
            } => self.transfer_for_loop(condition_statements, condition, body, post, after),

            Statement::Break { .. } => loop_context
                .map(|context| context.break_liveness)
                .unwrap_or(ScratchLiveness::LIVE),
            Statement::Continue { .. } => loop_context
                .map(|context| context.continue_liveness)
                .unwrap_or(ScratchLiveness::LIVE),
            Statement::Leave { .. } => self.exit_liveness,

            Statement::Return { offset, length } | Statement::Revert { offset, length } => {
                Self::range_read_liveness(self.static_extent(offset), self.static_extent(length))
            }
            Statement::Stop | Statement::Invalid | Statement::SelfDestruct { .. } => {
                ScratchLiveness::DEAD
            }
            // These outlined terminators read only bytes they first write
            // themselves (plus the free-pointer word at 0x40, outside scratch).
            Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. } => ScratchLiveness::DEAD,

            Statement::ExternalCall {
                args_offset,
                args_length,
                ..
            } => {
                // The callee reads the argument range; the return-data region is
                // written only up to the actual return data size, so it never
                // counts as a must-write.
                let reads = Self::range_read_liveness(
                    self.static_extent(args_offset),
                    self.static_extent(args_length),
                );
                after.join(reads)
            }
            Statement::Create { offset, length, .. } => {
                let reads = Self::range_read_liveness(
                    self.static_extent(offset),
                    self.static_extent(length),
                );
                after.join(reads)
            }
            Statement::Log { offset, length, .. } => {
                let reads = Self::range_read_liveness(
                    self.static_extent(offset),
                    self.static_extent(length),
                );
                after.join(reads)
            }

            // These copies write the full destination range (short sources are
            // zero-padded; return-data over-reads revert instead of returning).
            Statement::CodeCopy {
                destination,
                length,
                ..
            }
            | Statement::ExtCodeCopy {
                destination,
                length,
                ..
            }
            | Statement::ReturnDataCopy {
                destination,
                length,
                ..
            }
            | Statement::DataCopy {
                destination,
                length,
                ..
            }
            | Statement::CallDataCopy {
                destination,
                length,
                ..
            } => {
                let kills = Self::range_write_kills(
                    self.static_extent(destination),
                    self.static_extent(length),
                );
                after.minus(kills)
            }

            Statement::Block(region) => {
                self.process_statements(&mut region.statements, after, loop_context)
            }
        }
    }

    /// Backward transfer of an expression evaluated at this point.
    fn transfer_expression(
        &mut self,
        expression: &Expression,
        after: ScratchLiveness,
    ) -> ScratchLiveness {
        match expression {
            Expression::MLoad { offset, region } => {
                after.join(self.memory_load_liveness(offset, *region))
            }
            Expression::Keccak256 { offset, length } => {
                let reads = Self::range_read_liveness(
                    self.static_extent(offset),
                    self.static_extent(length),
                );
                after.join(reads)
            }
            // Non-candidate fused keccaks keep their helper call, whose
            // write-back fully overwrites the staged words.
            Expression::Keccak256Pair { .. } => after.minus(ScratchLiveness::LIVE),
            Expression::Keccak256Single { .. } => after.minus(ScratchLiveness {
                word0: true,
                word1: false,
            }),
            // See the `MappingSStore` note in `transfer_statement`.
            Expression::MappingSLoad { .. } => after,
            Expression::Call { function, .. } => {
                if let Some(record) = self.call_site_liveness.as_deref_mut() {
                    record
                        .entry(*function)
                        .and_modify(|liveness| *liveness = liveness.join(after))
                        .or_insert(after);
                }
                match self.summaries.get(function) {
                    Some(summary) => summary.apply(after),
                    None => ScratchLiveness::LIVE,
                }
            }
            _ => after,
        }
    }

    /// Backward fixpoint over a loop's cyclic liveness, then (in rewrite mode)
    /// one rewrite descent with the stabilized states.
    ///
    /// Execution order is `condition_statements → condition → body → post →
    /// condition_statements → …`, with `break` leaving to `after` and
    /// `continue` entering the post region. Starting from the all-dead seed,
    /// the monotone transfers converge in a handful of rounds.
    fn transfer_for_loop(
        &mut self,
        condition_statements: &mut Vec<Statement>,
        condition: &Expression,
        body: &mut crate::ir::Region,
        post: &mut crate::ir::Region,
        after: ScratchLiveness,
    ) -> ScratchLiveness {
        let exit_state = after;

        // Candidates inside the loop must not be folded during the fixpoint
        // iterations, so the analysis rounds run with rewriting disabled.
        let rewrite = self.rewrite.take();

        let mut at_condition = ScratchLiveness::DEAD;
        let (body_entry, post_entry) = loop {
            let post_entry = self.process_statements(
                &mut post.statements,
                at_condition,
                Some(LoopContext {
                    break_liveness: exit_state,
                    continue_liveness: at_condition,
                }),
            );
            let body_entry = self.process_statements(
                &mut body.statements,
                post_entry,
                Some(LoopContext {
                    break_liveness: exit_state,
                    continue_liveness: post_entry,
                }),
            );
            let after_condition = self.transfer_expression(condition, body_entry.join(exit_state));
            let next_at_condition =
                self.process_statements(condition_statements, after_condition, None);
            if next_at_condition == at_condition {
                break (body_entry, post_entry);
            }
            at_condition = next_at_condition;
        };

        self.rewrite = rewrite;
        if self.rewrite.is_some() {
            self.process_statements(
                &mut post.statements,
                at_condition,
                Some(LoopContext {
                    break_liveness: exit_state,
                    continue_liveness: at_condition,
                }),
            );
            self.process_statements(
                &mut body.statements,
                post_entry,
                Some(LoopContext {
                    break_liveness: exit_state,
                    continue_liveness: post_entry,
                }),
            );
            let after_condition = self.transfer_expression(condition, body_entry.join(exit_state));
            self.process_statements(condition_statements, after_condition, None);
        }

        at_condition
    }

    /// Liveness contributed by a full-word memory load.
    fn memory_load_liveness(&self, offset: &Value, region: MemoryRegion) -> ScratchLiveness {
        match self.static_extent(offset) {
            extent @ (StaticExtent::Known(_)
            | StaticExtent::Huge
            | StaticExtent::FreePointerDerived) => {
                Self::range_read_liveness(extent, StaticExtent::Known(BYTE_LENGTH_WORD as u64))
            }
            StaticExtent::Dynamic => match region {
                // Region tags record a translation-time constant classification:
                // dynamic allocations start at 0x80 and the free-pointer word is
                // [0x40, 0x60), both disjoint from scratch.
                MemoryRegion::Dynamic | MemoryRegion::FreePointerSlot => ScratchLiveness::DEAD,
                MemoryRegion::Scratch | MemoryRegion::Unknown => ScratchLiveness::LIVE,
            },
        }
    }

    /// Classifies a value as a compile-time offset/length.
    fn static_extent(&self, value: &Value) -> StaticExtent {
        match self.constants.get(&value.id.0) {
            Some(constant) => match constant.to_u64() {
                Some(number) => StaticExtent::Known(number),
                None => StaticExtent::Huge,
            },
            None if self.free_pointer_derived.contains(&value.id.0) => {
                StaticExtent::FreePointerDerived
            }
            None => StaticExtent::Dynamic,
        }
    }

    /// Scratch words a read of `[offset, offset + length)` may observe.
    ///
    /// A huge constant offset lies past scratch; a read whose end would wrap
    /// traps (PVM) or exhausts gas on memory expansion (EVM) before any byte is
    /// observed, so it contributes nothing either.
    fn range_read_liveness(offset: StaticExtent, length: StaticExtent) -> ScratchLiveness {
        if length == StaticExtent::Known(0) {
            return ScratchLiveness::DEAD;
        }
        match offset {
            StaticExtent::Huge | StaticExtent::FreePointerDerived => ScratchLiveness::DEAD,
            StaticExtent::Dynamic => ScratchLiveness::LIVE,
            StaticExtent::Known(offset) => {
                if offset >= SCRATCH_END {
                    return ScratchLiveness::DEAD;
                }
                let end = match length {
                    StaticExtent::Known(length) => offset.saturating_add(length),
                    StaticExtent::Huge
                    | StaticExtent::FreePointerDerived
                    | StaticExtent::Dynamic => u64::MAX,
                };
                ScratchLiveness {
                    word0: offset < SCRATCH_WORD1_OFFSET,
                    word1: end > SCRATCH_WORD1_OFFSET,
                }
            }
        }
    }

    /// Scratch words fully covered by a must-write of `[destination, destination + length)`.
    fn range_write_kills(destination: StaticExtent, length: StaticExtent) -> ScratchLiveness {
        let (StaticExtent::Known(destination), StaticExtent::Known(length)) = (destination, length)
        else {
            return ScratchLiveness::DEAD;
        };
        let end = destination.saturating_add(length);
        ScratchLiveness {
            word0: destination == 0 && end >= SCRATCH_WORD1_OFFSET,
            word1: destination <= SCRATCH_WORD1_OFFSET && end >= SCRATCH_END,
        }
    }
}

/// Compile-time facts about one object's values, gathered in a single walk.
struct ObjectFacts {
    /// Constant bindings, including the hashes that constant-operand fused
    /// keccaks will fold to.
    constants: BTreeMap<u32, BigUint>,
    /// Values provably `>= 0x60` at runtime: constants at or above `0x60` and
    /// free-memory-pointer loads, closed under `add`, trusted `and` masks,
    /// copies, branch outputs, and calls. Reads through them never observe
    /// scratch `[0, 0x40)`, and writes through them never corrupt the FMP
    /// slot `[0x40, 0x60)`.
    free_pointer_derived: BTreeSet<u32>,
    /// Whether the FMP slot provably holds a trusted pointer (`>= 0x60`)
    /// throughout: every direct store to it writes a trusted value, and no
    /// dynamic store, byte store, copy, or call return range can reach the
    /// slot through an untrusted destination. When false, `mload(0x40)` may
    /// yield a scratch offset and `free_pointer_derived` must not be used.
    free_pointer_slot_intact: bool,
}

/// Runtime floor of a trusted pointer.
///
/// `0x60` rather than the end of scratch (`0x40`): a pointer in `[0x40, 0x60)`
/// would let derived *writes* overwrite the FMP slot itself and break the
/// derivation invariant, while `>= 0x60` keeps every derived read out of
/// scratch and every derived write past the slot. solc's `0x60` empty-bytes
/// sentinel and `0x80` memoryguard base both qualify.
const TRUSTED_POINTER_FLOOR: u64 = 0x60;

/// Whether `and`-masking with this constant keeps a trusted pointer at or
/// above [`TRUSTED_POINTER_FLOOR`].
///
/// Two mask shapes qualify:
/// - High masks that only clear bits below bit 5, i.e. flooring to a multiple
///   of at most `0x20` (solc's alignment masking `and(x, not(31))`); a value
///   `>= 0x60` floored to a `0x20` granule stays `>= 0x60`.
/// - All-ones low masks `2^k - 1` for `k >= 32`. These are the width
///   truncations `guard_narrow` inserts after a dominating range guard has
///   proven `value <= mask`, so at runtime they are the identity. A
///   hand-written `and` of an FMP value that actually exceeds the mask can
///   only be built by first growing the FMP past `2^k` through engineered
///   pointer arithmetic — the same deliberately accepted gap as wrapping
///   `add`s (see the module documentation).
fn mask_preserves_free_pointer_range(mask: &BigUint) -> bool {
    let max_u256 = (BigUint::from(1u32) << 256u32) - BigUint::from(1u32);
    if (max_u256 ^ mask).to_u64().is_some_and(|low| low <= 0x1f) {
        return true;
    }
    let low_mask_width = mask.bits();
    low_mask_width >= 32 && *mask == (BigUint::from(1u32) << low_mask_width) - BigUint::from(1u32)
}

/// Optimistic interprocedural assumptions the facts fixpoint refines.
struct ProvenanceAssumptions {
    /// Parameters that receive a trusted pointer argument at every call site
    /// seen so far.
    trusted_parameters: BTreeSet<u32>,
    /// Per function, which return slots yield a trusted pointer on every
    /// returning path.
    trusted_returns: BTreeMap<FunctionId, Vec<bool>>,
    /// Loop-carried bindings (`For` loop variables, post-region inputs, and
    /// outputs) whose every incoming value is a trusted pointer: solc's
    /// memory copy loops advance a pointer as a loop variable.
    trusted_loop_values: BTreeSet<u32>,
}

/// Collects every `For`-bound value id for the optimistic loop-trust seed.
fn seed_loop_values(statements: &[Statement], seed: &mut BTreeSet<u32>) {
    for statement in statements {
        match statement {
            Statement::For {
                loop_variables,
                condition_statements,
                body,
                post_input_variables,
                post,
                outputs,
                ..
            } => {
                seed.extend(loop_variables.iter().map(|id| id.0));
                seed.extend(post_input_variables.iter().map(|id| id.0));
                seed.extend(outputs.iter().map(|id| id.0));
                seed_loop_values(condition_statements, seed);
                seed_loop_values(&body.statements, seed);
                seed_loop_values(&post.statements, seed);
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                seed_loop_values(&then_region.statements, seed);
                if let Some(region) = else_region {
                    seed_loop_values(&region.statements, seed);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    seed_loop_values(&case.body.statements, seed);
                }
                if let Some(region) = default {
                    seed_loop_values(&region.statements, seed);
                }
            }
            Statement::Block(region) => {
                seed_loop_values(&region.statements, seed);
            }
            _ => {}
        }
    }
}

/// Demotion targets of the innermost enclosing `For` during the facts walk.
#[derive(Clone, Copy)]
struct LoopTargets<'ids> {
    /// Receive `Break` values and the final loop variables.
    outputs: &'ids [ValueId],
    /// Receive body yields and `Continue` values.
    post_input_variables: &'ids [ValueId],
}

/// Records every compile-time fact about an object's values.
///
/// SSA ids are unique within an object and every use is dominated by its
/// definition, so a forward depth-first walk sees each operand's facts before
/// the expression that consumes it — including chained constant keccaks
/// (`keccak(a, keccak(b, c))`).
///
/// FMP provenance crosses function boundaries in solc output (allocation
/// helpers take and return pointers), so parameter and return-slot trust is
/// computed as a shrinking fixpoint: start from the optimistic assumption,
/// re-walk, and demote every parameter that some call site feeds a
/// non-derived argument and every return slot that some path returns a
/// non-derived value, until nothing changes. Functions without call sites
/// never run, so their optimistic parameters are harmless.
fn collect_facts(object: &Object) -> ObjectFacts {
    let mut loop_value_seed = BTreeSet::new();
    seed_loop_values(&object.code.statements, &mut loop_value_seed);
    for function in object.functions.values() {
        seed_loop_values(&function.body.statements, &mut loop_value_seed);
    }
    let mut assumptions = ProvenanceAssumptions {
        trusted_parameters: object
            .functions
            .values()
            .flat_map(|function| function.parameters.iter().map(|(id, _)| id.0))
            .collect(),
        trusted_returns: object
            .functions
            .iter()
            .map(|(id, function)| (*id, vec![true; function.returns.len()]))
            .collect(),
        trusted_loop_values: loop_value_seed,
    };

    loop {
        let mut seed = assumptions.trusted_parameters.clone();
        seed.extend(assumptions.trusted_loop_values.iter().copied());
        let mut facts = ObjectFacts {
            constants: BTreeMap::new(),
            free_pointer_derived: seed,
            free_pointer_slot_intact: true,
        };
        let mut collector = FactsCollector {
            object,
            facts: &mut facts,
            assumptions: &assumptions,
            demoted_parameters: BTreeSet::new(),
            demoted_returns: BTreeSet::new(),
            demoted_loop_values: BTreeSet::new(),
        };
        collector.walk_statements(&object.code.statements, None, None);
        for (id, function) in &object.functions {
            collector.walk_statements(&function.body.statements, Some(*id), None);
            // The fall-through return path uses the function's final return
            // value bindings.
            collector.check_return_values(*id, function.return_values.iter().copied());
        }

        let demoted_parameters = collector.demoted_parameters;
        let demoted_returns = collector.demoted_returns;
        let demoted_loop_values = collector.demoted_loop_values;
        // Only genuine assumption flips continue the fixpoint; re-observed
        // violations of already-demoted entries must not count as progress.
        let mut changed = false;
        for parameter in demoted_parameters {
            changed |= assumptions.trusted_parameters.remove(&parameter);
        }
        for (function, slot) in demoted_returns {
            if let Some(slots) = assumptions.trusted_returns.get_mut(&function) {
                if slots[slot] {
                    slots[slot] = false;
                    changed = true;
                }
            }
        }
        for value in demoted_loop_values {
            changed |= assumptions.trusted_loop_values.remove(&value);
        }
        if !changed {
            return facts;
        }
    }
}

/// One forward walk of the facts fixpoint.
struct FactsCollector<'walk> {
    object: &'walk Object,
    facts: &'walk mut ObjectFacts,
    assumptions: &'walk ProvenanceAssumptions,
    /// Parameters observed to receive a non-derived argument this round.
    demoted_parameters: BTreeSet<u32>,
    /// Return slots observed to yield a non-derived value this round.
    demoted_returns: BTreeSet<(FunctionId, usize)>,
    /// Loop-carried bindings observed to receive a non-derived value this round.
    demoted_loop_values: BTreeSet<u32>,
}

impl FactsCollector<'_> {
    /// Whether a value is known to stay out of scratch when used as a pointer:
    /// FMP-derived, or a constant of at least [`TRUSTED_POINTER_FLOOR`].
    fn is_pointer_trusted(&self, value: &Value) -> bool {
        self.facts.free_pointer_derived.contains(&value.id.0)
            || self
                .facts
                .constants
                .get(&value.id.0)
                .is_some_and(|constant| *constant >= BigUint::from(TRUSTED_POINTER_FLOOR))
    }

    /// Marks the FMP slot as breakable when a bulk write's destination range
    /// may reach `[0x40, 0x60)`.
    ///
    /// A constant destination is checked against the slot range (an unknown
    /// length writes upward from the destination, so anything starting below
    /// `0x60` may cover it). A dynamic destination is safe only when it is a
    /// trusted pointer (`>= 0x60`).
    fn check_slot_overwrite(&mut self, destination: &Value, length: Option<&Value>) {
        let destination_start = self
            .facts
            .constants
            .get(&destination.id.0)
            .and_then(BigUint::to_u64);
        let write_length = length.and_then(|length| {
            self.facts
                .constants
                .get(&length.id.0)
                .and_then(BigUint::to_u64)
        });
        let may_cover_slot = match destination_start {
            Some(start) => match write_length {
                Some(length) => length > 0 && start < 0x60 && start.saturating_add(length) > 0x40,
                None => start < 0x60,
            },
            None => match write_length {
                Some(0) => false,
                _ => !self.is_pointer_trusted(destination),
            },
        };
        if may_cover_slot {
            self.facts.free_pointer_slot_intact = false;
        }
    }

    /// Records call-argument trust violations for the callee's parameters and
    /// propagates return-slot trust to the call's bindings.
    fn visit_call(&mut self, function: &FunctionId, arguments: &[Value], bindings: &[ValueId]) {
        if let Some(callee) = self.object.functions.get(function) {
            for ((parameter, _), argument) in callee.parameters.iter().zip(arguments) {
                if !self.is_pointer_trusted(argument) {
                    self.demoted_parameters.insert(parameter.0);
                }
            }
        }
        let trusted_slots = self.assumptions.trusted_returns.get(function);
        for (slot, binding) in bindings.iter().enumerate() {
            let trusted =
                trusted_slots.is_some_and(|slots| slots.get(slot).copied().unwrap_or(false));
            if trusted {
                self.facts.free_pointer_derived.insert(binding.0);
            }
        }
    }

    /// Records return-slot trust violations for values leaving `function`.
    fn check_return_values(
        &mut self,
        function: FunctionId,
        return_values: impl Iterator<Item = ValueId>,
    ) {
        for (slot, value) in return_values.enumerate() {
            let trusted = self.facts.free_pointer_derived.contains(&value.0)
                || self
                    .facts
                    .constants
                    .get(&value.0)
                    .is_some_and(|constant| *constant >= BigUint::from(TRUSTED_POINTER_FLOOR));
            if !trusted {
                self.demoted_returns.insert((function, slot));
            }
        }
    }

    /// Examines an expression evaluated for effects (`Let` values, expression
    /// statements, and `For` conditions) for call-site provenance.
    fn visit_expression(&mut self, expression: &Expression, bindings: &[ValueId]) {
        if let Expression::Call {
            function,
            arguments,
        } = expression
        {
            self.visit_call(function, arguments, bindings);
        }
    }

    /// Marks a loop-carried binding as demoted when its incoming value is
    /// not a trusted pointer.
    fn check_loop_value(&mut self, target: ValueId, incoming: &Value) {
        if !self.is_pointer_trusted(incoming) {
            self.demoted_loop_values.insert(target.0);
        }
    }

    /// Forward walk populating [`ObjectFacts`] within one container.
    ///
    /// `loop_targets` names the innermost enclosing `For`'s demotion targets
    /// for `Break`/`Continue` values.
    fn walk_statements(
        &mut self,
        statements: &[Statement],
        function: Option<FunctionId>,
        loop_targets: Option<LoopTargets>,
    ) {
        for statement in statements {
            match statement {
                Statement::Let { bindings, value } => {
                    self.visit_expression(value, bindings);
                    if bindings.len() != 1 {
                        continue;
                    }
                    let binding = bindings[0].0;
                    let folded = match value {
                        Expression::Literal { value, .. } => Some(value.clone()),
                        Expression::Var(inner) => self.facts.constants.get(&inner.0).cloned(),
                        Expression::Keccak256Single { word0 } => self
                            .facts
                            .constants
                            .get(&word0.id.0)
                            .map(fold_keccak256_single),
                        Expression::Keccak256Pair { word0, word1 } => {
                            match (
                                self.facts.constants.get(&word0.id.0),
                                self.facts.constants.get(&word1.id.0),
                            ) {
                                (Some(word0), Some(word1)) => {
                                    Some(fold_keccak256_pair(word0, word1))
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    if let Some(constant) = folded {
                        self.facts.constants.insert(binding, constant);
                    }

                    let derived = match value {
                        Expression::MLoad { offset, region } => {
                            let resolved = self
                                .facts
                                .constants
                                .get(&offset.id.0)
                                .and_then(BigUint::to_u64);
                            region.is_free_pointer_slot(resolved)
                        }
                        Expression::Var(inner) => {
                            self.facts.free_pointer_derived.contains(&inner.0)
                        }
                        // Constants above the trusted floor are pointer roots
                        // too: solc's memoryguard base is the literal `0x80`,
                        // and inlined copy loops index off it directly.
                        Expression::Binary {
                            operation: crate::ir::BinaryOperation::Add,
                            lhs,
                            rhs,
                        } => self.is_pointer_trusted(lhs) || self.is_pointer_trusted(rhs),
                        Expression::Binary {
                            operation: crate::ir::BinaryOperation::And,
                            lhs,
                            rhs,
                        } => {
                            let masked = |derived: &Value, mask: &Value| {
                                self.is_pointer_trusted(derived)
                                    && self
                                        .facts
                                        .constants
                                        .get(&mask.id.0)
                                        .is_some_and(mask_preserves_free_pointer_range)
                            };
                            masked(lhs, rhs) || masked(rhs, lhs)
                        }
                        _ => false,
                    };
                    if derived {
                        self.facts.free_pointer_derived.insert(binding);
                    }
                }
                Statement::Expression(expression) => {
                    self.visit_expression(expression, &[]);
                }
                Statement::Leave { return_values } => {
                    if let Some(function) = function {
                        self.check_return_values(
                            function,
                            return_values.iter().map(|value| value.id),
                        );
                    }
                }
                Statement::MStore {
                    offset,
                    value,
                    region,
                } => {
                    let resolved = self
                        .facts
                        .constants
                        .get(&offset.id.0)
                        .and_then(BigUint::to_u64);
                    if region.is_free_pointer_slot(resolved) {
                        // The direct FMP update: the stored value must itself
                        // be a trusted pointer.
                        if !self.is_pointer_trusted(value) {
                            self.facts.free_pointer_slot_intact = false;
                        }
                    } else if resolved.is_none() && !self.is_pointer_trusted(offset) {
                        // A full-word store through an untracked dynamic
                        // offset could land on the slot.
                        self.facts.free_pointer_slot_intact = false;
                    }
                    // A *static* misaligned store overlapping the slot
                    // (solc's revert-string encoding) is benign unless its
                    // corruption reaches a later FMP read — that case is
                    // covered by the heap analysis corruption scan consulted
                    // alongside these facts.
                }
                // A byte store into the FMP word corrupts the pointer with an
                // arbitrary byte.
                Statement::MStore8 { offset, .. } => {
                    let resolved = self
                        .facts
                        .constants
                        .get(&offset.id.0)
                        .and_then(BigUint::to_u64);
                    match resolved {
                        Some(offset) => {
                            if (0x40..0x60).contains(&offset) {
                                self.facts.free_pointer_slot_intact = false;
                            }
                        }
                        None => {
                            if !self.is_pointer_trusted(offset) {
                                self.facts.free_pointer_slot_intact = false;
                            }
                        }
                    }
                }
                // Bulk writes into memory: the destination range must not be
                // able to touch the FMP slot.
                Statement::MCopy {
                    destination,
                    length,
                    ..
                } => {
                    self.check_slot_overwrite(destination, Some(length));
                }
                Statement::CodeCopy {
                    destination,
                    length,
                    ..
                }
                | Statement::ExtCodeCopy {
                    destination,
                    length,
                    ..
                }
                | Statement::ReturnDataCopy {
                    destination,
                    length,
                    ..
                }
                | Statement::DataCopy {
                    destination,
                    length,
                    ..
                }
                | Statement::CallDataCopy {
                    destination,
                    length,
                    ..
                } => {
                    self.check_slot_overwrite(destination, Some(length));
                }
                Statement::ExternalCall {
                    ret_offset,
                    ret_length,
                    ..
                } => {
                    self.check_slot_overwrite(ret_offset, Some(ret_length));
                }
                Statement::If {
                    inputs,
                    then_region,
                    else_region,
                    outputs,
                    ..
                } => {
                    self.walk_statements(&then_region.statements, function, loop_targets);
                    if let Some(region) = else_region {
                        self.walk_statements(&region.statements, function, loop_targets);
                    }
                    // An output is a trusted pointer when every value it can
                    // merge is one (a missing else yields `inputs` unchanged).
                    let else_values = else_region
                        .as_ref()
                        .map(|region| region.yields.as_slice())
                        .unwrap_or(inputs.as_slice());
                    for (index, output) in outputs.iter().enumerate() {
                        let trusted = then_region
                            .yields
                            .get(index)
                            .is_some_and(|value| self.is_pointer_trusted(value))
                            && else_values
                                .get(index)
                                .is_some_and(|value| self.is_pointer_trusted(value));
                        if trusted {
                            self.facts.free_pointer_derived.insert(output.0);
                        }
                    }
                }
                Statement::Switch {
                    inputs,
                    cases,
                    default,
                    outputs,
                    ..
                } => {
                    for case in cases {
                        self.walk_statements(&case.body.statements, function, loop_targets);
                    }
                    if let Some(region) = default {
                        self.walk_statements(&region.statements, function, loop_targets);
                    }
                    let default_values = default
                        .as_ref()
                        .map(|region| region.yields.as_slice())
                        .unwrap_or(inputs.as_slice());
                    for (index, output) in outputs.iter().enumerate() {
                        let trusted = cases.iter().all(|case| {
                            case.body
                                .yields
                                .get(index)
                                .is_some_and(|value| self.is_pointer_trusted(value))
                        }) && default_values
                            .get(index)
                            .is_some_and(|value| self.is_pointer_trusted(value));
                        if trusted {
                            self.facts.free_pointer_derived.insert(output.0);
                        }
                    }
                }
                Statement::For {
                    initial_values,
                    loop_variables,
                    condition_statements,
                    condition,
                    body,
                    post_input_variables,
                    post,
                    outputs,
                } => {
                    // Loop variables merge the initial values with the post
                    // region's yields; post inputs merge body yields with
                    // `Continue` values; outputs carry the final loop
                    // variables and `Break` values. All start optimistically
                    // trusted and are demoted on any untrusted incoming value.
                    for (variable, initial) in loop_variables.iter().zip(initial_values) {
                        self.check_loop_value(*variable, initial);
                    }
                    let targets = Some(LoopTargets {
                        outputs,
                        post_input_variables,
                    });
                    self.walk_statements(condition_statements, function, targets);
                    self.visit_expression(condition, &[]);
                    self.walk_statements(&body.statements, function, targets);
                    for (input, yielded) in post_input_variables.iter().zip(&body.yields) {
                        self.check_loop_value(*input, yielded);
                    }
                    self.walk_statements(&post.statements, function, targets);
                    for (variable, yielded) in loop_variables.iter().zip(&post.yields) {
                        self.check_loop_value(*variable, yielded);
                    }
                    for (output, variable) in outputs.iter().zip(loop_variables) {
                        if !self.facts.free_pointer_derived.contains(&variable.0) {
                            self.demoted_loop_values.insert(output.0);
                        }
                    }
                }
                Statement::Break { values } => {
                    if let Some(targets) = loop_targets {
                        for (output, value) in targets.outputs.iter().zip(values) {
                            self.check_loop_value(*output, value);
                        }
                    }
                }
                Statement::Continue { values } => {
                    if let Some(targets) = loop_targets {
                        for (input, value) in targets.post_input_variables.iter().zip(values) {
                            self.check_loop_value(*input, value);
                        }
                    }
                }
                Statement::Block(region) => {
                    self.walk_statements(&region.statements, function, loop_targets);
                }
                _ => {}
            }
        }
    }
}

/// Whether any fold candidate exists, for a cheap early exit.
fn contains_fold_candidate(object: &Object, constants: &BTreeMap<u32, BigUint>) -> bool {
    fn statements_contain_candidate(
        statements: &[Statement],
        constants: &BTreeMap<u32, BigUint>,
    ) -> bool {
        statements.iter().any(|statement| match statement {
            Statement::Let { bindings, value } if bindings.len() == 1 => match value {
                Expression::Keccak256Single { word0 } => constants.contains_key(&word0.id.0),
                Expression::Keccak256Pair { word0, word1 } => {
                    constants.contains_key(&word0.id.0) && constants.contains_key(&word1.id.0)
                }
                _ => false,
            },
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                statements_contain_candidate(&then_region.statements, constants)
                    || else_region.as_ref().is_some_and(|region| {
                        statements_contain_candidate(&region.statements, constants)
                    })
            }
            Statement::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|case| statements_contain_candidate(&case.body.statements, constants))
                    || default.as_ref().is_some_and(|region| {
                        statements_contain_candidate(&region.statements, constants)
                    })
            }
            Statement::For {
                condition_statements,
                body,
                post,
                ..
            } => {
                statements_contain_candidate(condition_statements, constants)
                    || statements_contain_candidate(&body.statements, constants)
                    || statements_contain_candidate(&post.statements, constants)
            }
            Statement::Block(region) => statements_contain_candidate(&region.statements, constants),
            _ => false,
        })
    }

    statements_contain_candidate(&object.code.statements, constants)
        || object
            .functions
            .values()
            .any(|function| statements_contain_candidate(&function.body.statements, constants))
}

/// Computes gen/kill transfer summaries for every function of one object.
///
/// Chaotic fixpoint iteration from the optimistic all-dead/all-killing seed;
/// both components only grow, so the iteration converges (recursion included).
/// Fold candidates transfer as identity — their write-back is decided later, so
/// neither their helper's overwrite nor a fold may be assumed here.
fn compute_function_summaries(
    object: &mut Object,
    facts: &ObjectFacts,
) -> BTreeMap<FunctionId, FunctionSummary> {
    let mut summaries: BTreeMap<FunctionId, FunctionSummary> = object
        .functions
        .keys()
        .map(|id| (*id, FunctionSummary::default()))
        .collect();

    loop {
        let mut changed = false;
        let function_ids: Vec<FunctionId> = object.functions.keys().copied().collect();
        for function_id in function_ids {
            let mut updated = FunctionSummary::default();
            for exit_live in [false, true] {
                let exit_liveness = if exit_live {
                    ScratchLiveness::LIVE
                } else {
                    ScratchLiveness::DEAD
                };
                let mut walker = ScratchLivenessWalker {
                    constants: &facts.constants,
                    free_pointer_derived: &facts.free_pointer_derived,
                    summaries: &summaries,
                    exit_liveness,
                    call_site_liveness: None,
                    rewrite: None,
                };
                let body = &mut object
                    .functions
                    .get_mut(&function_id)
                    .expect("function id enumerated above")
                    .body;
                let entry = walker.process_statements(&mut body.statements, exit_liveness, None);
                if exit_live {
                    updated.entry_if_exit_live = entry;
                } else {
                    updated.entry_if_exit_dead = entry;
                }
            }
            let current = summaries
                .get_mut(&function_id)
                .expect("summary seeded above");
            if *current != updated {
                *current = updated;
                changed = true;
            }
        }
        if !changed {
            return summaries;
        }
    }
}

/// Computes, for every function, the join of the liveness observed after each
/// of its call sites — the state its `Leave`/fall-through continues into.
///
/// The top-level code's fall-through ends execution, so its exit is all-dead.
/// Liveness values only grow across rounds; the iteration converges.
fn compute_exit_liveness(
    object: &mut Object,
    facts: &ObjectFacts,
    summaries: &BTreeMap<FunctionId, FunctionSummary>,
) -> BTreeMap<FunctionId, ScratchLiveness> {
    let mut exit_liveness: BTreeMap<FunctionId, ScratchLiveness> = object
        .functions
        .keys()
        .map(|id| (*id, ScratchLiveness::DEAD))
        .collect();

    loop {
        let mut observed: BTreeMap<FunctionId, ScratchLiveness> = BTreeMap::new();

        let mut walker = ScratchLivenessWalker {
            constants: &facts.constants,
            free_pointer_derived: &facts.free_pointer_derived,
            summaries,
            exit_liveness: ScratchLiveness::DEAD,
            call_site_liveness: Some(&mut observed),
            rewrite: None,
        };
        walker.process_statements(&mut object.code.statements, ScratchLiveness::DEAD, None);

        let function_ids: Vec<FunctionId> = object.functions.keys().copied().collect();
        for function_id in function_ids {
            let exit = *exit_liveness
                .get(&function_id)
                .expect("exit liveness seeded above");
            let mut walker = ScratchLivenessWalker {
                constants: &facts.constants,
                free_pointer_derived: &facts.free_pointer_derived,
                summaries,
                exit_liveness: exit,
                call_site_liveness: Some(&mut observed),
                rewrite: None,
            };
            let body = &mut object
                .functions
                .get_mut(&function_id)
                .expect("function id enumerated above")
                .body;
            walker.process_statements(&mut body.statements, exit, None);
        }

        let mut changed = false;
        for (function_id, liveness) in exit_liveness.iter_mut() {
            let seen = observed
                .get(function_id)
                .copied()
                .unwrap_or(ScratchLiveness::DEAD);
            let joined = liveness.join(seen);
            if joined != *liveness {
                *liveness = joined;
                changed = true;
            }
        }
        if !changed {
            return exit_liveness;
        }
    }
}

/// Folds constant `Keccak256Single` and `Keccak256Pair` expressions in an
/// object tree, preserving the helpers' scratch write-back where it is
/// observable.
///
/// Runs after the `mem_opt` fusion that creates the fused nodes, and again
/// after every simplify/inline round that can surface new constant operands.
/// See the module documentation for the liveness guard.
pub fn fold_constant_keccak(object: &mut Object) {
    // The FMP-derived offset classification is only sound while nothing can
    // plant an untrusted value in the FMP slot. `collect_facts` proves that
    // for every write it can attribute (direct slot stores, dynamic stores,
    // byte stores, copies, call return ranges). The one vector it cannot
    // judge locally — a *static* misaligned word store overlapping the slot,
    // which solc's revert-string encoding emits routinely and benignly — is
    // covered by the heap analysis' observed-corruption scan.
    let mut heap_analysis = crate::heap_opt::HeapAnalysis::new();
    heap_analysis.analyze_object(object);
    let corruption_free = !heap_analysis.fmp_corruption_observed();
    fold_object_tree(object, corruption_free);
}

/// Folds one object and recurses into its subobjects.
fn fold_object_tree(object: &mut Object, corruption_free: bool) {
    let mut facts = collect_facts(object);
    if !(corruption_free && facts.free_pointer_slot_intact) {
        facts.free_pointer_derived.clear();
    }
    if contains_fold_candidate(object, &facts.constants) {
        fold_object(object, &facts);
    }
    for subobject in &mut object.subobjects {
        fold_object_tree(subobject, corruption_free);
    }
}

/// Runs the three analysis/rewrite phases on a single object.
fn fold_object(object: &mut Object, facts: &ObjectFacts) {
    let summaries = compute_function_summaries(object, facts);
    let exit_liveness = compute_exit_liveness(object, facts, &summaries);

    let mut rewrite = RewriteState {
        next_value_id: ValueId(object.find_max_value_id() + 1),
        folded: 0,
        write_back_stores: 0,
    };

    let mut walker = ScratchLivenessWalker {
        constants: &facts.constants,
        free_pointer_derived: &facts.free_pointer_derived,
        summaries: &summaries,
        exit_liveness: ScratchLiveness::DEAD,
        call_site_liveness: None,
        rewrite: Some(&mut rewrite),
    };
    walker.process_statements(&mut object.code.statements, ScratchLiveness::DEAD, None);

    let function_ids: Vec<FunctionId> = object.functions.keys().copied().collect();
    for function_id in function_ids {
        let exit = *exit_liveness
            .get(&function_id)
            .expect("every function has an exit liveness entry");
        let mut walker = ScratchLivenessWalker {
            constants: &facts.constants,
            free_pointer_derived: &facts.free_pointer_derived,
            summaries: &summaries,
            exit_liveness: exit,
            call_site_liveness: None,
            rewrite: Some(&mut rewrite),
        };
        let body = &mut object
            .functions
            .get_mut(&function_id)
            .expect("function id enumerated above")
            .body;
        walker.process_statements(&mut body.statements, exit, None);
    }

    if rewrite.folded > 0 {
        log::debug!(
            "keccak_fold: object `{}`: folded {} constant keccak(s), emitted {} write-back store(s)",
            object.name,
            rewrite.folded,
            rewrite.write_back_stores,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BinaryOperation, Block, Function, Region, SwitchCase};

    fn let_literal(id: u32, value: u64) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Literal {
                value: BigUint::from(value),
                value_type: Type::Int(BitWidth::I256),
            },
        }
    }

    fn let_pair(id: u32, word0: u32, word1: u32) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Keccak256Pair {
                word0: Value::int(ValueId(word0)),
                word1: Value::int(ValueId(word1)),
            },
        }
    }

    fn let_single(id: u32, word0: u32) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Keccak256Single {
                word0: Value::int(ValueId(word0)),
            },
        }
    }

    fn let_mload(id: u32, offset: u32) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::MLoad {
                offset: Value::int(ValueId(offset)),
                region: MemoryRegion::Unknown,
            },
        }
    }

    fn sstore(key: u32, value: u32) -> Statement {
        Statement::SStore {
            key: Value::int(ValueId(key)),
            value: Value::int(ValueId(value)),
            static_slot: None,
        }
    }

    fn object_with_code(statements: Vec<Statement>) -> Object {
        let mut object = Object::new("test".to_string());
        object.code.statements = statements;
        object
    }

    fn count_mstores(statements: &[Statement]) -> usize {
        let mut count = 0;
        crate::ir::for_each_statement(statements, &mut |statement| {
            if matches!(statement, Statement::MStore { .. }) {
                count += 1;
            }
        });
        count
    }

    fn binding_literal(statements: &[Statement], id: u32) -> Option<BigUint> {
        let mut result = None;
        crate::ir::for_each_statement(statements, &mut |statement| {
            if let Statement::Let { bindings, value } = statement {
                if bindings.len() == 1 && bindings[0].0 == id {
                    if let Expression::Literal { value, .. } = value {
                        result = Some(value.clone());
                    }
                }
            }
        });
        result
    }

    #[test]
    fn folds_bare_when_scratch_dead() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_pair(2, 0, 1),
            sstore(2, 0),
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        let expected = fold_keccak256_pair(&BigUint::from(7u64), &BigUint::from(0u64));
        assert_eq!(binding_literal(&object.code.statements, 2), Some(expected));
        assert_eq!(count_mstores(&object.code.statements), 0);
    }

    #[test]
    fn write_back_when_scratch_read_later() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_pair(2, 0, 1),
            sstore(2, 0),
            let_mload(3, 1),
            sstore(3, 3),
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        let expected = fold_keccak256_pair(&BigUint::from(7u64), &BigUint::from(0u64));
        assert_eq!(binding_literal(&object.code.statements, 2), Some(expected));
        // The mload at constant offset 0 reads only scratch word 0, so exactly
        // one write-back store is emitted.
        assert_eq!(count_mstores(&object.code.statements), 1);
    }

    #[test]
    fn full_overwrite_kills_write_back() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_literal(4, 32),
            let_pair(2, 0, 1),
            // Both scratch words are overwritten before the read below.
            Statement::MStore {
                offset: Value::int(ValueId(1)),
                value: Value::int(ValueId(0)),
                region: MemoryRegion::Scratch,
            },
            Statement::MStore {
                offset: Value::int(ValueId(4)),
                value: Value::int(ValueId(0)),
                region: MemoryRegion::Scratch,
            },
            let_mload(3, 1),
            sstore(3, 3),
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        // Only the two pre-existing stores remain: the fold added none.
        assert_eq!(count_mstores(&object.code.statements), 2);
        assert!(binding_literal(&object.code.statements, 2).is_some());
    }

    #[test]
    fn exit_liveness_propagates_from_caller() {
        let mut function = Function::new(FunctionId(0), "hash_slot".to_string());
        function.body = Block {
            statements: vec![
                let_literal(10, 7),
                let_literal(11, 0),
                let_pair(12, 10, 11),
                sstore(12, 10),
            ],
        };

        let mut object = object_with_code(vec![
            Statement::Expression(Expression::Call {
                function: FunctionId(0),
                arguments: Vec::new(),
            }),
            let_literal(0, 0),
            let_mload(1, 0),
            sstore(1, 1),
            Statement::Stop,
        ]);
        object.functions.insert(FunctionId(0), function);

        fold_constant_keccak(&mut object);

        let function = &object.functions[&FunctionId(0)];
        assert!(binding_literal(&function.body.statements, 12).is_some());
        // The caller reads scratch word 0 after the call returns, so the fold
        // inside the callee must write the hashed key back.
        assert_eq!(count_mstores(&function.body.statements), 1);
        assert_eq!(count_mstores(&object.code.statements), 0);
    }

    #[test]
    fn chained_constant_keccaks_both_fold() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 6),
            let_pair(2, 0, 1),
            let_literal(3, 9),
            let_pair(4, 3, 2),
            sstore(4, 0),
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        let inner = fold_keccak256_pair(&BigUint::from(7u64), &BigUint::from(6u64));
        let outer = fold_keccak256_pair(&BigUint::from(9u64), &inner);
        assert_eq!(binding_literal(&object.code.statements, 2), Some(inner));
        assert_eq!(binding_literal(&object.code.statements, 4), Some(outer));
        assert_eq!(count_mstores(&object.code.statements), 0);
    }

    #[test]
    fn later_candidate_write_back_covers_earlier_fold() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 6),
            let_pair(2, 0, 1),
            sstore(2, 0),
            let_literal(3, 9),
            let_literal(4, 5),
            let_pair(5, 3, 4),
            sstore(5, 3),
            let_mload(6, 1),
            sstore(6, 6),
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        // Note: ValueId(1) is bound to 6, so the mload reads offset 6 and
        // touches both scratch words. The later candidate writes both back;
        // the earlier one is thereby fully covered and folds bare.
        assert!(binding_literal(&object.code.statements, 2).is_some());
        assert!(binding_literal(&object.code.statements, 5).is_some());
        assert_eq!(count_mstores(&object.code.statements), 2);
    }

    #[test]
    fn single_word_fold_ignores_word1_liveness() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 32),
            let_single(2, 0),
            sstore(2, 0),
            // Reads scratch word 1 only, which a fused single never writes.
            let_mload(3, 1),
            sstore(3, 3),
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        let expected = fold_keccak256_single(&BigUint::from(7u64));
        assert_eq!(binding_literal(&object.code.statements, 2), Some(expected));
        assert_eq!(count_mstores(&object.code.statements), 0);
    }

    #[test]
    fn return_range_keeps_scratch_live() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_literal(4, 64),
            let_pair(2, 0, 1),
            sstore(2, 0),
            Statement::Return {
                offset: Value::int(ValueId(1)),
                length: Value::int(ValueId(4)),
            },
        ]);
        fold_constant_keccak(&mut object);

        assert!(binding_literal(&object.code.statements, 2).is_some());
        // return(0, 64) reads both scratch words.
        assert_eq!(count_mstores(&object.code.statements), 2);
    }

    #[test]
    fn zero_length_return_is_dead() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_pair(2, 0, 1),
            sstore(2, 0),
            Statement::Return {
                offset: Value::int(ValueId(1)),
                length: Value::int(ValueId(1)),
            },
        ]);
        // ValueId(1) is the literal 0: return(0, 0) reads nothing.
        fold_constant_keccak(&mut object);

        assert!(binding_literal(&object.code.statements, 2).is_some());
        assert_eq!(count_mstores(&object.code.statements), 0);
    }

    #[test]
    fn dynamic_read_in_loop_body_keeps_scratch_live() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_pair(2, 0, 1),
            sstore(2, 0),
            let_literal(3, 1),
            Statement::For {
                initial_values: Vec::new(),
                loop_variables: Vec::new(),
                condition_statements: Vec::new(),
                condition: Expression::Var(ValueId(3)),
                body: Region {
                    statements: vec![let_mload(4, 1), sstore(4, 4)],
                    yields: Vec::new(),
                },
                post_input_variables: Vec::new(),
                post: Region::new(),
                outputs: Vec::new(),
            },
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        assert!(binding_literal(&object.code.statements, 2).is_some());
        // The loop body reads scratch word 0 on every iteration.
        assert_eq!(count_mstores(&object.code.statements), 1);
    }

    #[test]
    fn branch_join_keeps_scratch_live() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_pair(2, 0, 1),
            sstore(2, 0),
            let_literal(3, 1),
            Statement::If {
                condition: Value::int(ValueId(3)),
                inputs: Vec::new(),
                then_region: Region {
                    statements: vec![let_mload(4, 1), sstore(4, 4)],
                    yields: Vec::new(),
                },
                else_region: None,
                outputs: Vec::new(),
            },
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        assert!(binding_literal(&object.code.statements, 2).is_some());
        assert_eq!(count_mstores(&object.code.statements), 1);
    }

    #[test]
    fn switch_case_read_keeps_scratch_live() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_pair(2, 0, 1),
            sstore(2, 0),
            let_literal(3, 1),
            Statement::Switch {
                scrutinee: Value::int(ValueId(3)),
                inputs: Vec::new(),
                cases: vec![SwitchCase {
                    value: BigUint::from(1u64),
                    body: Region {
                        statements: vec![let_mload(4, 1), sstore(4, 4)],
                        yields: Vec::new(),
                    },
                }],
                default: None,
                outputs: Vec::new(),
            },
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        assert!(binding_literal(&object.code.statements, 2).is_some());
        assert_eq!(count_mstores(&object.code.statements), 1);
    }

    #[test]
    fn free_pointer_derived_return_folds_bare() {
        let mut object = object_with_code(vec![
            let_literal(0, 0x80),
            let_literal(1, 0x40),
            Statement::MStore {
                offset: Value::int(ValueId(1)),
                value: Value::int(ValueId(0)),
                region: MemoryRegion::FreePointerSlot,
            },
            let_literal(2, 7),
            let_literal(3, 0),
            let_pair(4, 2, 3),
            sstore(4, 2),
            Statement::Let {
                bindings: vec![ValueId(5)],
                value: Expression::MLoad {
                    offset: Value::int(ValueId(1)),
                    region: MemoryRegion::FreePointerSlot,
                },
            },
            let_literal(6, 0x20),
            Statement::Return {
                offset: Value::int(ValueId(5)),
                length: Value::int(ValueId(6)),
            },
        ]);
        fold_constant_keccak(&mut object);

        // The return range is FMP-derived (>= 0x80), so it cannot observe
        // scratch: the fold stays bare and only the FMP init store remains.
        assert!(binding_literal(&object.code.statements, 4).is_some());
        assert_eq!(count_mstores(&object.code.statements), 1);
    }

    #[test]
    fn untrusted_free_pointer_store_disables_provenance() {
        let mut object = object_with_code(vec![
            let_literal(0, 0),
            Statement::Let {
                bindings: vec![ValueId(1)],
                value: Expression::CallDataLoad {
                    offset: Value::int(ValueId(0)),
                },
            },
            let_literal(2, 0x40),
            // Plants an arbitrary calldata value in the FMP slot.
            Statement::MStore {
                offset: Value::int(ValueId(2)),
                value: Value::int(ValueId(1)),
                region: MemoryRegion::FreePointerSlot,
            },
            let_literal(3, 7),
            let_literal(4, 0),
            let_pair(5, 3, 4),
            sstore(5, 3),
            Statement::Let {
                bindings: vec![ValueId(6)],
                value: Expression::MLoad {
                    offset: Value::int(ValueId(2)),
                    region: MemoryRegion::FreePointerSlot,
                },
            },
            let_literal(7, 0x20),
            Statement::Return {
                offset: Value::int(ValueId(6)),
                length: Value::int(ValueId(7)),
            },
        ]);
        fold_constant_keccak(&mut object);

        // The FMP may now be a scratch offset, so the return range must be
        // assumed to read scratch: both write-back stores are emitted.
        assert!(binding_literal(&object.code.statements, 5).is_some());
        assert_eq!(count_mstores(&object.code.statements), 3);
    }

    #[test]
    fn loop_carried_pointer_read_folds_bare() {
        let mut object = object_with_code(vec![
            let_literal(0, 0x80),
            let_literal(1, 0x40),
            Statement::MStore {
                offset: Value::int(ValueId(1)),
                value: Value::int(ValueId(0)),
                region: MemoryRegion::FreePointerSlot,
            },
            let_literal(2, 7),
            let_literal(3, 0),
            let_pair(4, 2, 3),
            sstore(4, 2),
            Statement::Let {
                bindings: vec![ValueId(5)],
                value: Expression::MLoad {
                    offset: Value::int(ValueId(1)),
                    region: MemoryRegion::FreePointerSlot,
                },
            },
            let_literal(6, 1),
            Statement::For {
                initial_values: vec![Value::int(ValueId(5))],
                loop_variables: vec![ValueId(7)],
                condition_statements: Vec::new(),
                condition: Expression::Var(ValueId(6)),
                body: Region {
                    statements: vec![
                        // Reads through the loop-carried pointer, then
                        // advances it.
                        let_mload(8, 7),
                        sstore(8, 8),
                        let_literal(9, 0x20),
                        Statement::Let {
                            bindings: vec![ValueId(10)],
                            value: Expression::Binary {
                                operation: BinaryOperation::Add,
                                lhs: Value::int(ValueId(7)),
                                rhs: Value::int(ValueId(9)),
                            },
                        },
                    ],
                    yields: vec![Value::int(ValueId(10))],
                },
                post_input_variables: vec![ValueId(11)],
                post: Region {
                    statements: Vec::new(),
                    yields: vec![Value::int(ValueId(11))],
                },
                outputs: vec![ValueId(12)],
            },
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        // The loop variable stays a trusted pointer (fmp, fmp + 0x20, ...),
        // so the load through it cannot observe scratch.
        assert!(binding_literal(&object.code.statements, 4).is_some());
        assert_eq!(count_mstores(&object.code.statements), 1);
    }

    #[test]
    fn non_candidate_helper_kills_liveness() {
        let mut object = object_with_code(vec![
            let_literal(0, 7),
            let_literal(1, 0),
            let_pair(2, 0, 1),
            sstore(2, 0),
            // Runtime-operand pair: stays a helper call, whose write-back
            // covers both scratch words.
            let_mload(3, 1),
            Statement::Let {
                bindings: vec![ValueId(4)],
                value: Expression::Keccak256Pair {
                    word0: Value::int(ValueId(3)),
                    word1: Value::int(ValueId(3)),
                },
            },
            sstore(4, 3),
            let_mload(5, 1),
            sstore(5, 5),
            Statement::Stop,
        ]);
        fold_constant_keccak(&mut object);

        // The first mload keeps the candidate's write-back alive (one store);
        // the final mload after the runtime pair is covered by the helper.
        assert!(binding_literal(&object.code.statements, 2).is_some());
        assert!(binding_literal(&object.code.statements, 4).is_none());
        assert_eq!(count_mstores(&object.code.statements), 1);
    }
}
