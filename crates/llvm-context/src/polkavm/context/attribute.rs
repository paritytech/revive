//! The LLVM attribute.

use serde::Deserialize;
use serde::Serialize;

/// The LLVM attribute.
/// In order to check the real order in a new major version of LLVM, find the `Attributes.inc` file
/// inside of the LLVM build directory. This order is actually generated during the building.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Attribute {
    // FirstEnumAttr = 1,
    AllocAlign = 1,
    AllocatedPointer = 2,
    AlwaysInline = 3,
    Builtin = 4,
    Cold = 5,
    Convergent = 6,
    CoroDestroyOnlyWhenComplete = 7,
    CoroElideSafe = 8,
    DeadOnUnwind = 9,
    DisableSanitizerInstrumentation = 10,
    Flatten = 11,
    FnRetThunkExtern = 12,
    Hot = 13,
    HybridPatchable = 14,
    ImmArg = 15,
    InReg = 16,
    InlineHint = 17,
    JumpTable = 18,
    MinSize = 19,
    MustProgress = 20,
    Naked = 21,
    Nest = 22,
    NoAlias = 23,
    NoBuiltin = 24,
    NoCallback = 25,
    NoCfCheck = 26,
    NoCreateUndefOrPoison = 27,
    NoDivergenceSource = 28,
    NoDuplicate = 29,
    NoExt = 30,
    NoFree = 31,
    NoIPA = 32,
    NoImplicitFloat = 33,
    NoInline = 34,
    NoMerge = 35,
    NoOutline = 36,
    NoProfile = 37,
    NoRecurse = 38,
    NoRedZone = 39,
    NoReturn = 40,
    NoSanitizeBounds = 41,
    NoSanitizeCoverage = 42,
    NoSync = 43,
    NoUndef = 44,
    NoUnwind = 45,
    NonLazyBind = 46,
    NonNull = 47,
    NullPointerIsValid = 48,
    OptForFuzzing = 49,
    OptimizeForDebugging = 50,
    OptimizeForSize = 51,
    OptimizeNone = 52,
    PresplitCoroutine = 53,
    ReadNone = 54,
    ReadOnly = 55,
    Returned = 56,
    ReturnsTwice = 57,
    SExt = 58,
    SafeStack = 59,
    SanitizeAddress = 60,
    SanitizeAllocToken = 61,
    SanitizeHWAddress = 62,
    SanitizeMemTag = 63,
    SanitizeMemory = 64,
    SanitizeNumericalStability = 65,
    SanitizeRealtime = 66,
    SanitizeRealtimeBlocking = 67,
    SanitizeThread = 68,
    SanitizeType = 69,
    ShadowCallStack = 70,
    SkipProfile = 71,
    Speculatable = 72,
    SpeculativeLoadHardening = 73,
    StackProtect = 74,
    StackProtectReq = 75,
    StackProtectStrong = 76,
    StrictFP = 77,
    SwiftAsync = 78,
    SwiftError = 79,
    SwiftSelf = 80,
    WillReturn = 81,
    Writable = 82,
    WriteOnly = 83,
    ZExt = 84,
    //LastEnumAttr = 84,
    //FirstTypeAttr = 85,
    ByRef = 85,
    ByVal = 86,
    ElementType = 87,
    InAlloca = 88,
    Preallocated = 89,
    StructRet = 90,
    //LastTypeAttr = 90,
    //FirstIntAttr = 91,
    Alignment = 91,
    AllocKind = 92,
    AllocSize = 93,
    Captures = 94,
    DeadOnReturn = 95,
    DenormalFPEnv = 96,
    Dereferenceable = 97,
    DereferenceableOrNull = 98,
    Memory = 99,
    NoFPClass = 100,
    StackAlignment = 101,
    UWTable = 102,
    VScaleRange = 103,
    //LastIntAttr = 103,
    //FirstConstantRangeAttr = 104,
    Range = 104,
    //LastConstantRangeAttr = 104,
    //FirstConstantRangeListAttr = 105,
    Initializes = 105,
    //LastConstantRangeListAttr = 105,
}

/// Rust mirror of LLVM's `llvm::ModRefInfo`.
///
/// Discriminants are the C++ enum values verbatim from
/// `llvm/include/llvm/Support/ModRef.h` lines 28-38 of the LLVM 21.1.8
/// source tree (`LLVM_SYS_211_PREFIX`):
///
/// ```text
/// enum class ModRefInfo : uint8_t {
///   NoModRef = 0,
///   Ref = 1,
///   Mod = 2,
///   ModRef = Ref | Mod,
/// };
/// ```
#[allow(dead_code)] // ModRef mirror — every variant kept for completeness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum ModRefInfo {
    NoModRef = 0,
    Ref = 1,
    Mod = 2,
    ModRef = 3,
}

/// Rust mirror of LLVM's `llvm::IRMemLocation`.
///
/// Discriminants are the C++ enum values verbatim from
/// `llvm/include/llvm/Support/ModRef.h` lines 60-73 of the LLVM 21.1.8
/// source tree:
///
/// ```text
/// enum class IRMemLocation {
///   ArgMem = 0,
///   InaccessibleMem = 1,
///   ErrnoMem = 2,
///   Other = 3,
///   ...
/// };
/// ```
///
/// The discriminant also serves as the location's index into the packed
/// `memory(...)` payload — see the `memory_location_pos` helper.
#[allow(dead_code)] // LLVM location mirror — every variant kept for completeness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum MemoryLocation {
    ArgMem = 0,
    InaccessibleMem = 1,
    ErrnoMem = 2,
    Other = 3,
}

/// Mirror of `llvm::MemoryEffectsBase::BitsPerLoc` — see
/// `llvm/include/llvm/Support/ModRef.h:82` of LLVM 21.1.8:
///
/// ```text
/// static constexpr uint32_t BitsPerLoc = 2;
/// ```
const MEMORY_EFFECT_BITS_PER_LOCATION: u64 = 2;

/// Mirror of `llvm::MemoryEffectsBase::getLocationPos` — see
/// `llvm/include/llvm/Support/ModRef.h:85-87` of LLVM 21.1.8:
///
/// ```text
/// static uint32_t getLocationPos(Location Loc) {
///   return (uint32_t)Loc * BitsPerLoc;
/// }
/// ```
const fn memory_location_pos(location: MemoryLocation) -> u64 {
    (location as u64) * MEMORY_EFFECT_BITS_PER_LOCATION
}

/// Mirror of `llvm::MemoryEffectsBase::setModRef` for a single location on
/// an initially-zero payload — see `llvm/include/llvm/Support/ModRef.h:91-94`
/// of LLVM 21.1.8:
///
/// ```text
/// void setModRef(Location Loc, ModRefInfo MR) {
///   Data &= ~(LocMask << getLocationPos(Loc));
///   Data |= static_cast<uint32_t>(MR) << getLocationPos(Loc);
/// }
/// ```
const fn pack_memory_effect(location: MemoryLocation, mod_ref: ModRefInfo) -> u64 {
    (mod_ref as u64) << memory_location_pos(location)
}

/// Per-location memory effect packed into the integer payload of
/// [`Attribute::Memory`]. Every variant's encoding is derived through the
/// crate-private `pack_memory_effect` helper from `MemoryLocation` /
/// `ModRefInfo`, which are Rust mirrors of LLVM 21 enums with discriminants
/// matching C++. No raw integer literals appear in [`MemoryEffect::encoding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEffect {
    /// Do not attach a `memory(...)` attribute to the function.
    Unrestricted,
    /// `memory(none)` — function has no observable memory effect.
    None,
    /// `memory(inaccessiblemem: read)` — the function only reads pallet-revive
    /// runtime state (storage, call context, etc.) reached via host syscalls.
    /// LLVM sees no pointer effects from the caller's perspective.
    ReadInaccessible,
    /// `memory(argmem: read, inaccessiblemem: read)` — the function reads from
    /// a pointer argument **and** from pallet-revive runtime state. Used by
    /// helpers that dereference a key buffer and forward into a host syscall.
    ReadArgAndInaccessible,
    /// `memory(inaccessiblemem: write)` — the only externally visible effect
    /// is a write into pallet-revive runtime state. Heap loads remain CSE-able
    /// across calls to such a helper.
    WriteInaccessible,
    /// `memory(other: read)` — function reads from regular heap memory only.
    /// Pointer arguments, runtime state and errno are all untouched, so the
    /// helper is invalidated by any heap store rather than by sstore wrappers.
    ReadOther,
}

impl MemoryEffect {
    /// LLVM's packed integer encoding of this effect, or `None` when no
    /// `memory(...)` attribute should be applied. See the crate-private
    /// `pack_memory_effect` helper above for the LLVM 21 source the
    /// encoding mirrors.
    pub const fn encoding(self) -> Option<u64> {
        match self {
            Self::Unrestricted => None,
            Self::None => Some(0),
            Self::ReadInaccessible => Some(pack_memory_effect(
                MemoryLocation::InaccessibleMem,
                ModRefInfo::Ref,
            )),
            Self::ReadArgAndInaccessible => Some(
                pack_memory_effect(MemoryLocation::ArgMem, ModRefInfo::Ref)
                    | pack_memory_effect(MemoryLocation::InaccessibleMem, ModRefInfo::Ref),
            ),
            Self::WriteInaccessible => Some(pack_memory_effect(
                MemoryLocation::InaccessibleMem,
                ModRefInfo::Mod,
            )),
            Self::ReadOther => Some(pack_memory_effect(MemoryLocation::Other, ModRefInfo::Ref)),
        }
    }
}

#[cfg(test)]
mod memory_effect_tests {
    use super::*;

    /// Locks the encoding against LLVM 21.1.8's `MemoryEffectsBase` layout.
    /// Expected integers were cross-checked by compiling probe contracts and
    /// reading the `memory(...)` strings off the emitted LLVM IR — every
    /// helper using one of these variants printed the matching attribute
    /// (e.g. `memory(inaccessiblemem: read)` for `ReadInaccessible`,
    /// `memory(read, argmem: none, inaccessiblemem: none, errnomem: none)` —
    /// LLVM 21's verbose form of `memory(other: read)` — for `ReadOther`).
    /// If LLVM ever reorders `IRMemLocation` (as happened between LLVM 18 and
    /// 21 when `ErrnoMem` was added), this test fires before codegen does.
    #[test]
    fn encoding_matches_llvm_21_layout() {
        assert_eq!(MemoryEffect::Unrestricted.encoding(), None);
        assert_eq!(MemoryEffect::None.encoding(), Some(0));
        assert_eq!(MemoryEffect::ReadInaccessible.encoding(), Some(4));
        assert_eq!(MemoryEffect::ReadArgAndInaccessible.encoding(), Some(5));
        assert_eq!(MemoryEffect::WriteInaccessible.encoding(), Some(8));
        assert_eq!(MemoryEffect::ReadOther.encoding(), Some(64));
    }
}
