use indexmap::IndexMap;
use petgraph::prelude::*;

use crate::{
    cfg::{Branch, Program},
    instruction::Instruction,
    symbol::Kind,
};

use super::BlockAnalysis;
