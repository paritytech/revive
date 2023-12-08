use indexmap::IndexSet;
use primitive_types::U256;

use crate::POINTER_SIZE;

#[derive(Debug, Default)]
pub struct SymbolTable {
    symbols: IndexSet<Symbol>,
    nonce: usize,
}

impl SymbolTable {
    fn next(&mut self) -> usize {
        let current = self.nonce;
        self.nonce += 1;
        current
    }

    pub fn temporary(&mut self, type_hint: Option<Type>) -> Symbol {
        let id = self.next();
        let symbol = Symbol {
            address: Address::Temporary(id),
            type_hint: type_hint.unwrap_or_default(),
            kind: Kind::Value,
        };
        assert!(self.symbols.insert(symbol));

        symbol
    }

    pub fn constant(&mut self, value: U256, type_hint: Option<Type>) -> Symbol {
        let symbol = Symbol {
            address: Address::Constant(value),
            type_hint: type_hint.unwrap_or_default(),
            kind: Kind::Value,
        };
        self.symbols.insert(symbol);

        symbol
    }

    pub fn global(&mut self, label: Global) -> Symbol {
        let symbol = Symbol {
            address: Address::Label(label),
            type_hint: label.typ(),
            kind: label.kind(),
        };
        self.symbols.insert(symbol);

        symbol
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct Symbol {
    pub address: Address,
    pub type_hint: Type,
    pub kind: Kind,
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({} {})", self.type_hint, self.address)?;

        match self.kind {
            Kind::Pointer => write!(f, "*"),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Address {
    Constant(U256),
    Temporary(usize),
    Label(Global),
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Constant(value) => write!(f, "0x{value:02x}"),
            Self::Temporary(n) => write!(f, "tmp_{n}"),
            Self::Label(label) => write!(f, "{label:?}"),
        }
    }
}

impl Address {
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        Self::Constant(U256::from_big_endian(bytes))
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Default, Clone, Copy)]
pub enum Type {
    #[default]
    Word,
    UInt(usize),
    Int(usize),
    Bytes(usize),
    Bool,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Word => write!(f, "word"),
            Self::UInt(size) => write!(f, "u{}", size),
            Self::Int(size) => write!(f, "i{}", size),
            Self::Bytes(size) => write!(f, "bytes{size}"),
            Self::Bool => write!(f, "bool"),
        }
    }
}

impl Type {
    pub fn pointer() -> Self {
        Self::UInt(POINTER_SIZE)
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Kind {
    Pointer,
    Value,
    Function,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Global {
    Stack,
    StackHeight,

    CallData,
    Memory,
    ReturnData,

    MemoryCopy,

    // EVM runtime environment
    Sha3,
    Address,
    CallDataLoad,
    CallDataSize,
    CallDataCopy,
    CodeSize,
    CodeCopy,
    GasPrice,
    ExtCodeSize,
    ExtCodeCopy,
    ReturnDataSize,
    ReturnDataCopy,
    ExtCodeHash,
    BlockHash,
    Coinbase,
    Timestamp,
    BlockNumber,
    PrevRanDao,
    GasLimit,
    ChainId,
    SelfBalance,
    BaseFee,
    SLoad,
    SStore,
    Gas,
    Create,
    Create2,
    Call,
    StaticCall,
    DelegateCall,
    CallCode,
    Return,
    Stop,
    Revert,
    SelfDestruct,
    Event,
}

impl Global {
    pub fn typ(&self) -> Type {
        match self {
            Self::Stack | Self::CallData | Self::Memory | Self::ReturnData => Type::pointer(),
            Self::StackHeight => Type::UInt(POINTER_SIZE),
            _ => Type::Word,
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Self::Stack | Self::CallData | Self::Memory | Self::ReturnData => Kind::Pointer,
            Self::StackHeight => Kind::Value,
            _ => Kind::Function,
        }
    }
}
