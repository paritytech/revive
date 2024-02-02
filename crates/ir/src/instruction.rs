use crate::symbol::{Global, SymbolRef};
use std::fmt::Write;

#[derive(PartialEq, Debug)]
pub enum Instruction {
    Nop,

    /// `x = y op z`
    BinaryAssign {
        x: SymbolRef,
        y: SymbolRef,
        operator: Operator,
        z: SymbolRef,
    },

    /// `x = op y`
    UnaryAssign {
        x: SymbolRef,
        operator: Operator,
        y: SymbolRef,
    },

    /// `branch target`
    UncoditionalBranch {
        target: SymbolRef,
    },

    /// `branch target if condition`
    ConditionalBranch {
        condition: SymbolRef,
        target: SymbolRef,
    },

    /// `call(label, n)`
    Procedure {
        symbol: Global,
        parameters: Vec<SymbolRef>,
    },

    /// `x = call(label, n)`
    Function {
        symbol: Global,
        x: SymbolRef,
        parameters: Vec<SymbolRef>,
    },

    /// `x = y`
    Copy {
        x: SymbolRef,
        y: SymbolRef,
    },

    /// `x[index] = y`
    IndexedAssign {
        x: SymbolRef,
        index: SymbolRef,
        y: SymbolRef,
    },

    /// `x = y[index]`
    IndexedCopy {
        x: SymbolRef,
        y: SymbolRef,
        index: SymbolRef,
    },
}

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BinaryAssign { x, y, operator, z } => write!(f, "{x} = {y} {operator:?} {z}"),

            Self::UnaryAssign { x, operator, y } => write!(f, "{x} = {operator:?} {y} "),

            Self::UncoditionalBranch { target } => write!(f, "branch {target}"),

            Self::ConditionalBranch { condition, target } => {
                write!(f, "if {condition} branch {target}")
            }

            Self::Procedure { symbol, parameters } => write!(
                f,
                "{symbol:?}({})",
                parameters.iter().fold(String::new(), |mut acc, p| {
                    write!(&mut acc, "{p}, ").unwrap();
                    acc
                })
            ),

            Self::Function {
                symbol,
                x,
                parameters: args,
            } => write!(
                f,
                "{x} = {symbol:?}({})",
                args.iter().fold(String::new(), |mut acc, p| {
                    write!(&mut acc, "{p}, ").unwrap();
                    acc
                })
            ),

            Self::Copy { x, y } => write!(f, "{x} = {y}"),

            Self::IndexedAssign { x, index, y } => write!(f, "{x}[{index}] = {y}"),

            Self::IndexedCopy { x, y, index } => write!(f, "{x} = {y}[{index}]"),

            Self::Nop => write!(f, "no-op"),
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum Operator {
    Add,
    Mul,
    Sub,
    Div,
    SDiv,
    Mod,
    SMod,
    AddMod,
    MulMod,
    Exp,
    SignExtend,

    LessThan,
    GreaterThan,
    SignedLessThan,
    SignedGreaterThan,
    Equal,
    IsZero,

    And,
    Or,
    Xor,
    Not,
    Byte,
    ShiftLeft,
    ShiftRight,
    ShiftArithmeticRight,
}
