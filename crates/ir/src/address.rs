use primitive_types::U256;

#[derive(Clone, Copy)]
pub enum Kind {
    Constant(U256),
    Temporary(usize),
    Stack,
}

#[derive(Clone, Copy)]
pub struct Address {
    pub kind: Kind,
    pub type_hint: Option<Type>,
}

impl From<(Kind, Option<Type>)> for Address {
    fn from(value: (Kind, Option<Type>)) -> Self {
        Self {
            kind: value.0,
            type_hint: value.1,
        }
    }
}

impl Address {
    pub fn new(kind: Kind, type_hint: Option<Type>) -> Self {
        Self { kind, type_hint }
    }
}

#[derive(Clone, Copy)]
pub enum Type {
    Int { size: u16 },
    Bytes { size: u8 },
    Bool,
}

impl Type {
    pub fn int(size: u16) -> Self {
        Self::Int { size }
    }

    fn bytes(size: u8) -> Self {
        Self::Bytes { size }
    }
}

impl Default for Type {
    fn default() -> Self {
        Type::Bytes { size: 32 }
    }
}

pub enum LinearMemory {
    CallData,
    Memory,
    ReturnData,
}
