//! The address space aliases.

/// The address space aliases.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressSpace {
    /// The stack memory.
    #[default]
    Stack,
    /// The heap memory.
    Heap,
    /// The auxiliary heap memory.
    HeapAuxiliary,
    /// The generic memory page.
    Generic,
    /// The code area.
    Code,
    /// The storage.
    Storage,
    /// The transient storage.
    TransientStorage,
}

impl From<AddressSpace> for inkwell::AddressSpace {
    fn from(value: AddressSpace) -> Self {
        match value {
            AddressSpace::Stack => Self::from(0),
            AddressSpace::Heap => Self::from(1),
            AddressSpace::HeapAuxiliary => Self::from(2),
            AddressSpace::Generic => Self::from(3),
            AddressSpace::Code => Self::from(4),
            AddressSpace::Storage => Self::from(5),
            AddressSpace::TransientStorage => Self::from(6),
        }
    }
}
