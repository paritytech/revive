//! The address space aliases.

/// The address space aliases.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressSpace {
    /// The stack memory.
    #[default]
    Stack,
    /// The heap memory.
    Heap,
}

impl From<AddressSpace> for inkwell::AddressSpace {
    fn from(value: AddressSpace) -> Self {
        match value {
            AddressSpace::Stack => Self::from(0),
            AddressSpace::Heap => Self::from(1),
        }
    }
}
