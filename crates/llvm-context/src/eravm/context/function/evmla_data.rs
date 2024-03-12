//!
//! The LLVM function EVM legacy assembly data.
//!

use std::collections::BTreeMap;

use crate::eravm::context::function::block::evmla_data::key::Key as BlockKey;
use crate::eravm::context::function::block::Block;

///
/// The LLVM function EVM legacy assembly data.
///
/// Describes some data that is only relevant to the EVM legacy assembly.
///
#[derive(Debug)]
pub struct EVMLAData<'ctx> {
    /// The ordinary blocks with numeric tags.
    /// Is only used by the Solidity EVM compiler.
    pub blocks: BTreeMap<BlockKey, Vec<Block<'ctx>>>,
    /// The function stack size.
    pub stack_size: usize,
}

impl<'ctx> EVMLAData<'ctx> {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(stack_size: usize) -> Self {
        Self {
            blocks: BTreeMap::new(),
            stack_size,
        }
    }

    ///
    /// Inserts a function block.
    ///
    pub fn insert_block(&mut self, key: BlockKey, block: Block<'ctx>) {
        if let Some(blocks) = self.blocks.get_mut(&key) {
            blocks.push(block);
        } else {
            self.blocks.insert(key, vec![block]);
        }
    }

    ///
    /// Returns the block with the specified tag and initial stack pattern.
    ///
    /// If there is only one block, it is returned unconditionally.
    ///
    pub fn find_block(
        &self,
        key: &BlockKey,
        stack_hash: &md5::Digest,
    ) -> anyhow::Result<Block<'ctx>> {
        if self
            .blocks
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("Undeclared function block {}", key))?
            .len()
            == 1
        {
            return self
                .blocks
                .get(key)
                .ok_or_else(|| anyhow::anyhow!("Undeclared function block {}", key))?
                .first()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Undeclared function block {}", key));
        }

        self.blocks
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("Undeclared function block {}", key))?
            .iter()
            .find(|block| {
                block
                    .evm()
                    .stack_hashes
                    .iter()
                    .any(|hash| hash == stack_hash)
            })
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Undeclared function block {}", key))
    }
}
