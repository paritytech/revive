use indexmap::IndexMap;
use petgraph::prelude::NodeIndex;
use primitive_types::U256;
use std::{cell::RefCell, rc::Rc};

use crate::POINTER_SIZE;

#[derive(Debug, Default)]
pub struct SymbolTable {
    table: IndexMap<NodeIndex, IndexMap<usize, Rc<RefCell<Symbol>>>>,
    symbols: IndexMap<usize, Rc<RefCell<Symbol>>>,
    global_scope: NodeIndex,
    id_nonce: usize,
}

impl SymbolTable {
    pub fn merge_scopes(&mut self, node: NodeIndex, target: NodeIndex) {
        let sym = self.symbols.remove(&0).unwrap();
        let new = self
            .table
            .get(&NodeIndex::default())
            .unwrap()
            .get(&0)
            .unwrap();
        //RefCell::replace(&sym, Rc::clone(new));
    }

    pub fn get_symbol(&self, id: usize) -> SymbolRef {
        SymbolRef {
            inner: self.symbols.get(&id).unwrap().clone(),
            id,
        }
    }

    pub fn insert(&mut self, scope: NodeIndex, symbol: Symbol) -> SymbolRef {
        let id = self.next();
        let inner = Rc::new(RefCell::new(symbol));

        self.table
            .entry(scope)
            .or_default()
            .insert(id, Rc::clone(&inner));
        self.symbols.insert(id, inner.clone());

        SymbolRef { inner, id }
    }

    pub fn global(&mut self, label: Global) -> SymbolRef {
        self.table
            .entry(self.global_scope)
            .or_default()
            .iter()
            .find(|(_, symbol)| symbol.borrow().address == Address::Label(label))
            .map(|(id, _)| *id)
            .map(|id| self.get_symbol(id))
            .unwrap_or_else(|| self.insert(self.global_scope, Symbol::builder().global(label)))
    }

    pub fn temporary(&mut self, node: NodeIndex) -> SymbolRef {
        self.insert(node, Symbol::builder().temporary().variable().done())
    }

    fn next(&mut self) -> usize {
        let current = self.id_nonce;
        self.id_nonce += 1;
        current
    }
}

#[derive(Default)]
pub struct SymbolBuilder<A = (), K = ()> {
    address: A,
    type_hint: Type,
    kind: K,
}

impl<K> SymbolBuilder<(), K> {
    pub fn temporary(self) -> SymbolBuilder<Address, K> {
        SymbolBuilder {
            address: Address::Temporary,
            type_hint: self.type_hint,
            kind: self.kind,
        }
    }

    pub fn stack(self, slot: i32) -> SymbolBuilder<Address, K> {
        SymbolBuilder {
            address: Address::Stack(slot),
            type_hint: self.type_hint,
            kind: self.kind,
        }
    }

    pub fn global(self, label: Global) -> Symbol {
        Symbol {
            address: Address::Label(label),
            type_hint: label.typ(),
            kind: label.kind(),
        }
    }
}

impl<A> SymbolBuilder<A, ()> {
    pub fn constant(self, bytes: &[u8]) -> SymbolBuilder<A, Kind> {
        SymbolBuilder {
            address: self.address,
            type_hint: Type::Bytes(bytes.len()),
            kind: Kind::Constant(U256::from_big_endian(bytes)),
        }
    }

    pub fn variable(self) -> SymbolBuilder<A, Kind> {
        SymbolBuilder {
            address: self.address,
            type_hint: self.type_hint,
            kind: Kind::Variable,
        }
    }
}

impl<A, K> SymbolBuilder<A, K> {
    pub fn of(self, type_hint: Type) -> Self {
        Self { type_hint, ..self }
    }
}

impl SymbolBuilder<Address, Kind> {
    pub fn done(self) -> Symbol {
        Symbol {
            address: self.address,
            type_hint: self.type_hint,
            kind: self.kind,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct Symbol {
    pub address: Address,
    pub type_hint: Type,
    pub kind: Kind,
}

impl Symbol {
    pub fn builder() -> SymbolBuilder {
        Default::default()
    }
}

#[derive(Clone, Debug)]
pub struct SymbolRef {
    inner: Rc<RefCell<Symbol>>,
    id: usize,
}

impl std::fmt::Display for SymbolRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let symbol = self.symbol();

        let address = format!("${}_{}", self.id, symbol.address);

        match symbol.kind {
            Kind::Pointer => write!(f, "*{address}"),
            Kind::Constant(value) => {
                write!(f, "{} {address} := {value}", symbol.type_hint)
            }
            _ => write!(f, "{} {address} ", symbol.type_hint),
        }
    }
}

impl SymbolRef {
    pub fn replace_type(&self, type_hint: Type) {
        self.inner.replace_with(|old| Symbol {
            address: old.address,
            kind: old.kind,
            type_hint,
        });
    }

    pub fn symbol(&self) -> Symbol {
        *self.inner.borrow()
    }

    pub fn id(&self) -> usize {
        self.id
    }
}

impl PartialEq for SymbolRef {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Default, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Address {
    #[default]
    Temporary,
    Stack(i32),
    Label(Global),
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stack(slot) => write!(f, "stack[{slot}]"),
            Self::Temporary => write!(f, "tmp"),
            Self::Label(label) => write!(f, "{label:?}"),
        }
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

#[derive(Default, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Kind {
    Pointer,
    #[default]
    Variable,
    Constant(U256),
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
            Self::StackHeight => Kind::Variable,
            _ => Kind::Function,
        }
    }
}
