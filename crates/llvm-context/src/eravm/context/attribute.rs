//!
//! The LLVM attribute.
//!

use serde::Deserialize;
use serde::Serialize;

///
/// The LLVM attribute.
///
/// In order to check the real order in a new major version of LLVM, find the `Attribute.inc` file
/// inside of the LLVM build directory. This order is actually generated during the building.
///
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Attribute {
    /// Unused.
    Unused,
    /// The eponymous LLVM attribute.
    AllocAlign,
    /// The eponymous LLVM attribute.
    AllocatedPointer,
    /// The eponymous LLVM attribute.
    AlwaysInline,
    /// The eponymous LLVM attribute.
    ArgMemOnly,
    /// The eponymous LLVM attribute.
    Builtin,
    /// The eponymous LLVM attribute.
    Cold,
    /// The eponymous LLVM attribute.
    Convergent,
    /// The eponymous LLVM attribute.
    DisableSanitizerInstrumentation,
    /// The eponymous LLVM attribute.
    FnRetThunkExtern,
    /// The eponymous LLVM attribute.
    Hot,
    /// The eponymous LLVM attribute.
    ImmArg,
    /// The eponymous LLVM attribute.
    InReg,
    /// The eponymous LLVM attribute.
    InaccessibleMemOnly,
    /// The eponymous LLVM attribute.
    InaccessibleMemOrArgMemOnly,
    /// The eponymous LLVM attribute.
    InlineHint,
    /// The eponymous LLVM attribute.
    JumpTable,
    /// The eponymous LLVM attribute.
    MinSize,
    /// The eponymous LLVM attribute.
    MustProgress,
    /// The eponymous LLVM attribute.
    Naked,
    /// The eponymous LLVM attribute.
    Nest,
    /// The eponymous LLVM attribute.
    NoAlias,
    /// The eponymous LLVM attribute.
    NoBuiltin,
    /// The eponymous LLVM attribute.
    NoCallback,
    /// The eponymous LLVM attribute.
    NoCapture,
    /// The eponymous LLVM attribute.
    NoCfCheck,
    /// The eponymous LLVM attribute.
    NoDuplicate,
    /// The eponymous LLVM attribute.
    NoFree,
    /// The eponymous LLVM attribute.
    NoImplicitFloat,
    /// The eponymous LLVM attribute.
    NoInline,
    /// The eponymous LLVM attribute.
    NoMerge,
    /// The eponymous LLVM attribute.
    NoProfile,
    /// The eponymous LLVM attribute.
    NoRecurse,
    /// The eponymous LLVM attribute.
    NoRedZone,
    /// The eponymous LLVM attribute.
    NoReturn,
    /// The eponymous LLVM attribute.
    NoSanitizeBounds,
    /// The eponymous LLVM attribute.
    NoSanitizeCoverage,
    /// The eponymous LLVM attribute.
    NoSync,
    /// The eponymous LLVM attribute.
    NoUndef,
    /// The eponymous LLVM attribute.
    NoUnwind,
    /// The eponymous LLVM attribute.
    NonLazyBind,
    /// The eponymous LLVM attribute.
    NonNull,
    /// The eponymous LLVM attribute.
    NullPointerIsValid,
    /// The eponymous LLVM attribute.
    OptForFuzzing,
    /// The eponymous LLVM attribute.
    OptimizeForSize,
    /// The eponymous LLVM attribute.
    OptimizeNone,
    /// The eponymous LLVM attribute.
    PresplitCoroutine,
    /// The eponymous LLVM attribute.
    ReadNone,
    /// The eponymous LLVM attribute.
    ReadOnly,
    /// The eponymous LLVM attribute.
    Returned,
    /// The eponymous LLVM attribute.
    ReturnsTwice,
    /// The eponymous LLVM attribute.
    SExt,
    /// The eponymous LLVM attribute.
    SafeStack,
    /// The eponymous LLVM attribute.
    SanitizeAddress,
    /// The eponymous LLVM attribute.
    SanitizeHWAddress,
    /// The eponymous LLVM attribute.
    SanitizeMemTag,
    /// The eponymous LLVM attribute.
    SanitizeMemory,
    /// The eponymous LLVM attribute.
    SanitizeThread,
    /// The eponymous LLVM attribute.
    ShadowCallStack,
    /// The eponymous LLVM attribute.
    Speculatable,
    /// The eponymous LLVM attribute.
    SpeculativeLoadHardening,
    /// The eponymous LLVM attribute.
    StackProtect,
    /// The eponymous LLVM attribute.
    StackProtectReq,
    /// The eponymous LLVM attribute.
    StackProtectStrong,
    /// The eponymous LLVM attribute.
    StrictFP,
    /// The eponymous LLVM attribute.
    SwiftAsync,
    /// The eponymous LLVM attribute.
    SwiftError,
    /// The eponymous LLVM attribute.
    SwiftSelf,
    /// The eponymous LLVM attribute.
    WillReturn,
    /// The eponymous LLVM attribute.
    WriteOnly,
    /// The eponymous LLVM attribute.
    ZExt,
    /// The eponymous LLVM attribute.
    ByRef,
    /// The eponymous LLVM attribute.
    ByVal,
    /// The eponymous LLVM attribute.
    ElementType,
    /// The eponymous LLVM attribute.
    InAlloca,
    /// The eponymous LLVM attribute.
    Preallocated,
    /// The eponymous LLVM attribute.
    StructRet,
    /// The eponymous LLVM attribute.
    Alignment,
    /// The eponymous LLVM attribute.
    AllocKind,
    /// The eponymous LLVM attribute.
    AllocSize,
    /// The eponymous LLVM attribute.
    Dereferenceable,
    /// The eponymous LLVM attribute.
    DereferenceableOrNull,
    /// The eponymous LLVM attribute.
    StackAlignment,
    /// The eponymous LLVM attribute.
    UWTable,
    /// The eponymous LLVM attribute.
    VScaleRange,
}

impl TryFrom<&str> for Attribute {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "AlwaysInline" => Ok(Attribute::AlwaysInline),
            "Cold" => Ok(Attribute::Cold),
            "Hot" => Ok(Attribute::Hot),
            "MinSize" => Ok(Attribute::MinSize),
            "OptimizeForSize" => Ok(Attribute::OptimizeForSize),
            "NoInline" => Ok(Attribute::NoInline),
            "WillReturn" => Ok(Attribute::WillReturn),
            "WriteOnly" => Ok(Attribute::WriteOnly),
            "ReadNone" => Ok(Attribute::ReadNone),
            "ReadOnly" => Ok(Attribute::ReadOnly),
            "NoReturn" => Ok(Attribute::NoReturn),
            "InaccessibleMemOnly" => Ok(Attribute::InaccessibleMemOnly),
            "MustProgress" => Ok(Attribute::MustProgress),
            _ => Err(value.to_owned()),
        }
    }
}
