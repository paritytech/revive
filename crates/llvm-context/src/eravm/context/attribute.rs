//! The LLVM attribute.

use serde::Deserialize;
use serde::Serialize;

/// The LLVM attribute.
///
/// In order to check the real order in a new major version of LLVM, find the `Attribute.inc` file
/// inside of the LLVM build directory. This order is actually generated during the building.
///
/// FIXME: Generate this in build.rs?
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Attribute {
    /// Unused (attributes start at 1).
    Unused = 0,
    AllocAlign = 1,
    AllocatedPointer = 2,
    AlwaysInline = 3,
    Builtin = 4,
    Cold = 5,
    Convergent = 6,
    DisableSanitizerInstrumentation = 7,
    FnRetThunkExtern = 8,
    Hot = 9,
    ImmArg = 10,
    InReg = 11,
    InlineHint = 12,
    JumpTable = 13,
    MinSize = 14,
    MustProgress = 15,
    Naked = 16,
    Nest = 17,
    NoAlias = 18,
    NoBuiltin = 19,
    NoCallback = 20,
    NoCapture = 21,
    NoCfCheck = 22,
    NoDuplicate = 23,
    NoFree = 24,
    NoImplicitFloat = 25,
    NoInline = 26,
    NoMerge = 27,
    NoProfile = 28,
    NoRecurse = 29,
    NoRedZone = 30,
    NoReturn = 31,
    NoSanitizeBounds = 32,
    NoSanitizeCoverage = 33,
    NoSync = 34,
    NoUndef = 35,
    NoUnwind = 36,
    NonLazyBind = 37,
    NonNull = 38,
    NullPointerIsValid = 39,
    OptForFuzzing = 40,
    OptimizeForSize = 41,
    OptimizeNone = 42,
    PresplitCoroutine = 43,
    ReadNone = 44,
    ReadOnly = 45,
    Returned = 46,
    ReturnsTwice = 47,
    SExt = 48,
    SafeStack = 49,
    SanitizeAddress = 50,
    SanitizeHWAddress = 51,
    SanitizeMemTag = 52,
    SanitizeMemory = 53,
    SanitizeThread = 54,
    ShadowCallStack = 55,
    SkipProfile = 56,
    Speculatable = 57,
    SpeculativeLoadHardening = 58,
    StackProtect = 59,
    StackProtectReq = 60,
    StackProtectStrong = 61,
    StrictFP = 62,
    SwiftAsync = 63,
    SwiftError = 64,
    SwiftSelf = 65,
    WillReturn = 66,
    WriteOnly = 67,
    ZExt = 68,
    // FirstTypeAttr = 69,
    ByRef = 69,
    ByVal = 70,
    ElementType = 71,
    InAlloca = 72,
    Preallocated = 73,
    StructRet = 74,
    // LastTypeAttr = 74,
    // FirstIntAttr = 75,
    Alignment = 75,
    AllocKind = 76,
    AllocSize = 77,
    Dereferenceable = 78,
    DereferenceableOrNull = 79,
    Memory = 80,
    StackAlignment = 81,
    UWTable = 82,
    VScaleRange = 83,
    // LastIntAttr = 83,
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
            // FIXME: Not in Attributes.inc
            //"InaccessibleMemOnly" => Ok(Attribute::InaccessibleMemOnly),
            "MustProgress" => Ok(Attribute::MustProgress),
            _ => Err(value.to_owned()),
        }
    }
}
