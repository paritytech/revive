//! The address space aliases.

/// The address space aliases.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressSpace {
    /// The stack memory.
    #[default]
    Stack,
    /// The heap memory.
    Heap,
    /// The generic memory page.
    Storage,
    /// The transient storage.
    TransientStorage,
}

impl From<AddressSpace> for inkwell::AddressSpace {
    fn from(value: AddressSpace) -> Self {
        match value {
            AddressSpace::Stack => Self::from(0),
            AddressSpace::Heap => Self::from(1),
            AddressSpace::Storage => Self::from(5),
            AddressSpace::TransientStorage => Self::from(6),
        }
    }
}
