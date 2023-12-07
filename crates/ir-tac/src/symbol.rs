use indexmap::IndexSet;
use primitive_types::U256;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Kind {
    Constant(U256),
    Temporary(usize),
    Label(Global),
}

impl Kind {
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        Self::Constant(U256::from_big_endian(bytes))
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct Symbol {
    pub kind: Kind,
    pub type_hint: Type,
}

impl Symbol {
    fn global(symbol: Global) -> Self {
        let type_hint = match symbol {
            Global::StackHeight => Type::Int(4),
            _ => Default::default(),
        };

        Self::new(Kind::Label(symbol), type_hint)
    }

    fn new(kind: Kind, type_hint: Type) -> Self {
        Self { kind, type_hint }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Default, Clone, Copy)]
pub enum Type {
    #[default]
    Word,
    Int(usize),
    Bytes(usize),
    Bool,
}

impl Type {
    pub fn pointer() -> Self {
        Self::Int(4)
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Global {
    /// Pointer
    Stack,
    /// Stack height variable
    StackHeight,

    /// Pointer
    CallData,
    /// Pointer
    Memory,
    /// Pointer
    ReturnData,

    /// Low level `memcpy` like function
    MemoryCopy,

    // EVM
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
        let symbol = Symbol::new(Kind::Temporary(id), type_hint.unwrap_or_default());
        assert!(self.symbols.insert(symbol));

        symbol
    }

    pub fn constant(&mut self, value: U256, type_hint: Option<Type>) -> Symbol {
        let symbol = Symbol::new(Kind::Constant(value), type_hint.unwrap_or_default());
        self.symbols.insert(symbol);

        symbol
    }

    pub fn global(&mut self, label: Global) -> Symbol {
        let symbol = Symbol::global(label);
        self.symbols.insert(symbol);

        symbol
    }
}
