//!
//! The function name.
//!

use serde::Deserialize;
use serde::Serialize;

///
/// The function name.
///
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Name {
    /// The user-defined function.
    UserDefined(String),

    /// `x + y`
    Add,
    /// `x - y`
    Sub,
    /// `x * y`
    Mul,
    /// `x / y` or `0` if `y == 0`
    Div,
    /// `x % y` or `0` if `y == 0`
    Mod,
    /// `x / y`, for signed numbers in two’s complement, `0` if `y == 0`
    Sdiv,
    /// `x % y`, for signed numbers in two’s complement, `0` if `y == 0`
    Smod,

    /// `1` if `x < y`, `0` otherwise
    Lt,
    /// `1` if `x > y`, `0` otherwise
    Gt,
    /// `1` if `x == y`, `0` otherwise
    Eq,
    /// `1` if `x == 0`, `0` otherwise
    IsZero,
    /// `1` if `x < y`, `0` otherwise, for signed numbers in two’s complement
    Slt,
    /// `1` if `x > y`, `0` otherwise, for signed numbers in two’s complement
    Sgt,

    /// bitwise "or" of `x` and `y`
    Or,
    /// bitwise "xor" of `x` and `y`
    Xor,
    /// bitwise "not" of `x` (every bit of `x` is negated)
    Not,
    /// bitwise "and" of `x` and `y`
    And,
    /// logical shift left `y` by `x` bits
    Shl,
    /// logical shift right `y` by `x` bits
    Shr,
    /// signed arithmetic shift right `y` by `x` bits
    Sar,
    /// `n`th byte of `x`, where the most significant byte is the `0`th byte
    Byte,
    /// discard value x
    Pop,

    /// `(x + y) % m` with arbitrary precision arithmetic, `0` if `m == 0`
    AddMod,
    /// `(x * y) % m` with arbitrary precision arithmetic, `0` if `m == 0`
    MulMod,
    /// `x` to the power of `y`
    Exp,
    /// sign extend from `(i*8+7)`th bit counting from least significant
    SignExtend,

    /// `keccak(mem[p…(p+n)))`
    Keccak256,

    /// `mem[p…(p+32))`
    MLoad,
    /// `mem[p…(p+32)) := v`
    MStore,
    /// `mem[p] := v & 0xff` (only modifies a single byte)
    MStore8,
    /// heap memory copy
    MCopy,

    /// `storage[p]`
    SLoad,
    /// `storage[p] := v`
    SStore,
    /// transient `storage[p]`
    TLoad,
    /// transient `storage[p] := v`
    TStore,
    /// `loadimmutable` storage read
    LoadImmutable,
    /// `setimmutable` storage write
    SetImmutable,

    /// call data starting from position `p` (32 bytes)
    CallDataLoad,
    /// size of call data in bytes
    CallDataSize,
    /// copy `s` bytes from calldata at position `f` to memory at position `t`
    CallDataCopy,
    /// size of the code of the current contract / execution context
    CodeSize,
    /// copy `s` bytes from code at position `f` to mem at position `t`
    CodeCopy,
    /// size of the code at address `a`
    ExtCodeSize,
    /// code hash of address `a`
    ExtCodeHash,
    /// size of the last returndata
    ReturnDataSize,
    /// copy `s` bytes from returndata at position `f` to mem at position `t`
    ReturnDataCopy,

    /// end execution, return data `mem[p…(p+s))`
    Return,
    /// end execution, revert state changes, return data `mem[p…(p+s))`
    Revert,
    /// stop execution, identical to `return(0, 0)`
    Stop,
    /// end execution with invalid instruction
    Invalid,

    /// log without topics and data `mem[p…(p+s))`
    Log0,
    /// log with topic t1 and data `mem[p…(p+s))`
    Log1,
    /// log with topics t1, t2 and data `mem[p…(p+s))`
    Log2,
    /// log with topics t1, t2, t3 and data `mem[p…(p+s))`
    Log3,
    /// log with topics t1, t2, t3, t4 and data `mem[p…(p+s))`
    Log4,

    /// call contract at address a with input `mem[in…(in+insize))` providing `g` gas and `v` wei
    /// and output area `mem[out…(out+outsize))` returning 0 on error (e.g. out of gas)
    /// and 1 on success
    /// [See more](https://docs.soliditylang.org/en/v0.8.2/yul.html#yul-call-return-area)
    Call,
    /// identical to call but only use the code from a and stay in the context of the current
    /// contract otherwise
    CallCode,
    /// identical to `callcode` but also keeps `caller` and `callvalue`
    DelegateCall,
    /// identical to `call(g, a, 0, in, insize, out, outsize)` but do not allows state modifications
    StaticCall,

    /// create new contract with code `mem[p…(p+n))` and send `v` wei and return the new address
    ///
    /// Passes bytecode to the system contracts.
    Create,
    /// create new contract with code `mem[p…(p+n))` at address
    /// `keccak256(0xff . this . s . keccak256(mem[p…(p+n)))` and send `v` wei and return the
    /// new address, where `0xff` is a 1-byte value, this is the current contract’s address as a
    /// 20-byte value and `s` is a big-endian 256-bit value
    ///
    /// Passes bytecode to the system contracts.
    Create2,
    /// create new contract with code `mem[p…(p+n))` and send `v` wei and return the new address
    ///
    /// Passes hash to the system contracts.
    ZkCreate,
    /// create new contract with code `mem[p…(p+n))` at address
    /// `keccak256(0xff . this . s . keccak256(mem[p…(p+n)))` and send `v` wei and return the
    /// new address, where `0xff` is a 1-byte value, this is the current contract’s address as a
    /// 20-byte value and `s` is a big-endian 256-bit value
    ///
    /// Passes hash to the system contracts.
    ZkCreate2,
    /// returns the size in the data area
    DataSize,
    /// is equivalent to `CodeCopy`
    DataCopy,
    /// returns the offset in the data area
    DataOffset,

    /// `linkersymbol` is a stub call
    LinkerSymbol,
    /// `memoryguard` is a stub call
    MemoryGuard,

    /// address of the current contract / execution context
    Address,
    /// call sender (excluding `delegatecall`)
    Caller,

    /// wei sent together with the current call
    CallValue,
    /// gas still available to execution
    Gas,
    /// wei balance at address `a`
    Balance,
    /// equivalent to `balance(address())`, but cheaper
    SelfBalance,

    /// block gas limit of the current block
    GasLimit,
    /// gas price of the transaction
    GasPrice,
    /// transaction sender
    Origin,
    /// ID of the executing chain (EIP 1344)
    ChainId,
    /// current block number
    Number,
    /// timestamp of the current block in seconds since the epoch
    Timestamp,
    /// hash of block nr b - only for last 256 blocks excluding current
    BlockHash,
    /// versioned hash of transaction’s i-th blob
    BlobHash,
    /// difficulty of the current block
    Difficulty,
    /// https://eips.ethereum.org/EIPS/eip-4399
    Prevrandao,
    /// current mining beneficiary
    CoinBase,
    /// size of memory, i.e. largest accessed memory index
    MSize,

    /// verbatim instruction with 0 inputs and 0 outputs
    /// only works in the Yul mode, so it is mostly used as a tool for extending Yul for zkSync
    Verbatim {
        /// the number of input arguments
        input_size: usize,
        /// the number of output arguments
        output_size: usize,
    },

    /// current block’s base fee (EIP-3198 and EIP-1559)
    BaseFee,
    /// current block’s blob base fee (EIP-7516 and EIP-4844)
    BlobBaseFee,
    /// current position in code
    Pc,
    /// like `codecopy(t, f, s)` but take code at address `a`
    ExtCodeCopy,
    /// end execution, destroy current contract and send funds to `a`
    SelfDestruct,

    /// The eponymous EraVM Yul extension instruction.
    ZkToL1,
    /// The eponymous EraVM Yul extension instruction.
    ZkCodeSource,
    /// The eponymous EraVM Yul extension instruction.
    ZkPrecompile,
    /// The eponymous EraVM Yul extension instruction.
    ZkMeta,
    /// The eponymous EraVM Yul extension instruction.
    ZkSetContextU128,
    /// The eponymous EraVM Yul extension instruction.
    ZkSetPubdataPrice,
    /// The eponymous EraVM Yul extension instruction.
    ZkIncrementTxCounter,
    /// The eponymous EraVM Yul extension instruction.
    ZkEventInitialize,
    /// The eponymous EraVM Yul extension instruction.
    ZkEventWrite,

    /// The eponymous EraVM Yul extension instruction.
    ZkMimicCall,
    /// The eponymous EraVM Yul extension instruction.
    ZkSystemMimicCall,
    /// The eponymous EraVM Yul extension instruction.
    ZkMimicCallByRef,
    /// The eponymous EraVM Yul extension instruction.
    ZkSystemMimicCallByRef,
    /// The eponymous EraVM Yul extension instruction.
    ZkRawCall,
    /// The eponymous EraVM Yul extension instruction.
    ZkRawCallByRef,
    /// The eponymous EraVM Yul extension instruction.
    ZkSystemCall,
    /// The eponymous EraVM Yul extension instruction.
    ZkSystemCallByRef,
    /// The eponymous EraVM Yul extension instruction.
    ZkStaticRawCall,
    /// The eponymous EraVM Yul extension instruction.
    ZkStaticRawCallByRef,
    /// The eponymous EraVM Yul extension instruction.
    ZkStaticSystemCall,
    /// The eponymous EraVM Yul extension instruction.
    ZkStaticSystemCallByRef,
    /// The eponymous EraVM Yul extension instruction.
    ZkDelegateRawCall,
    /// The eponymous EraVM Yul extension instruction.
    ZkDelegateRawCallByRef,
    /// The eponymous EraVM Yul extension instruction.
    ZkDelegateSystemCall,
    /// The eponymous EraVM Yul extension instruction.
    ZkDelegateSystemCallByRef,

    /// The eponymous EraVM Yul extension instruction.
    ZkLoadCalldataIntoActivePtr,
    /// The eponymous EraVM Yul extension instruction.
    ZkLoadReturndataIntoActivePtr,
    /// The eponymous EraVM Yul extension instruction.
    ZkPtrAddIntoActive,
    /// The eponymous EraVM Yul extension instruction.
    ZkPtrShrinkIntoActive,
    /// The eponymous EraVM Yul extension instruction.
    ZkPtrPackIntoActive,

    /// The eponymous EraVM Yul extension instruction.
    ZkMultiplicationHigh,

    /// The eponymous EraVM Yul extension instruction.
    ZkGlobalLoad,
    /// The eponymous EraVM Yul extension instruction.
    ZkGlobalExtraAbiData,
    /// The eponymous EraVM Yul extension instruction.
    ZkGlobalStore,
}

impl Name {
    ///
    /// Tries parsing the verbatim instruction.
    ///
    fn parse_verbatim(input: &str) -> Option<Self> {
        let verbatim = input.strip_prefix("verbatim")?;
        let regex = regex::Regex::new(r"_(\d+)i_(\d+)o").expect("Always valid");
        let captures = regex.captures(verbatim)?;
        let input_size: usize = captures.get(1)?.as_str().parse().ok()?;
        let output_size: usize = captures.get(2)?.as_str().parse().ok()?;
        Some(Self::Verbatim {
            input_size,
            output_size,
        })
    }
}

impl From<&str> for Name {
    fn from(input: &str) -> Self {
        if let Some(verbatim) = Self::parse_verbatim(input) {
            return verbatim;
        }

        match input {
            "add" => Self::Add,
            "sub" => Self::Sub,
            "mul" => Self::Mul,
            "div" => Self::Div,
            "mod" => Self::Mod,
            "sdiv" => Self::Sdiv,
            "smod" => Self::Smod,

            "lt" => Self::Lt,
            "gt" => Self::Gt,
            "eq" => Self::Eq,
            "iszero" => Self::IsZero,
            "slt" => Self::Slt,
            "sgt" => Self::Sgt,

            "or" => Self::Or,
            "xor" => Self::Xor,
            "not" => Self::Not,
            "and" => Self::And,
            "shl" => Self::Shl,
            "shr" => Self::Shr,
            "sar" => Self::Sar,
            "byte" => Self::Byte,
            "pop" => Self::Pop,

            "addmod" => Self::AddMod,
            "mulmod" => Self::MulMod,
            "exp" => Self::Exp,
            "signextend" => Self::SignExtend,

            "keccak256" => Self::Keccak256,

            "mload" => Self::MLoad,
            "mstore" => Self::MStore,
            "mstore8" => Self::MStore8,
            "mcopy" => Self::MCopy,

            "sload" => Self::SLoad,
            "sstore" => Self::SStore,
            "tload" => Self::TLoad,
            "tstore" => Self::TStore,
            "loadimmutable" => Self::LoadImmutable,
            "setimmutable" => Self::SetImmutable,

            "calldataload" => Self::CallDataLoad,
            "calldatasize" => Self::CallDataSize,
            "calldatacopy" => Self::CallDataCopy,
            "codesize" => Self::CodeSize,
            "codecopy" => Self::CodeCopy,
            "returndatasize" => Self::ReturnDataSize,
            "returndatacopy" => Self::ReturnDataCopy,
            "extcodesize" => Self::ExtCodeSize,
            "extcodehash" => Self::ExtCodeHash,

            "return" => Self::Return,
            "revert" => Self::Revert,

            "log0" => Self::Log0,
            "log1" => Self::Log1,
            "log2" => Self::Log2,
            "log3" => Self::Log3,
            "log4" => Self::Log4,

            "call" => Self::Call,
            "delegatecall" => Self::DelegateCall,
            "staticcall" => Self::StaticCall,

            "create" => Self::Create,
            "create2" => Self::Create2,
            "$zk_create" => Self::ZkCreate,
            "$zk_create2" => Self::ZkCreate2,
            "datasize" => Self::DataSize,
            "dataoffset" => Self::DataOffset,
            "datacopy" => Self::DataCopy,

            "stop" => Self::Stop,
            "invalid" => Self::Invalid,

            "linkersymbol" => Self::LinkerSymbol,
            "memoryguard" => Self::MemoryGuard,

            "address" => Self::Address,
            "caller" => Self::Caller,

            "callvalue" => Self::CallValue,
            "gas" => Self::Gas,
            "balance" => Self::Balance,
            "selfbalance" => Self::SelfBalance,

            "gaslimit" => Self::GasLimit,
            "gasprice" => Self::GasPrice,
            "origin" => Self::Origin,
            "chainid" => Self::ChainId,
            "timestamp" => Self::Timestamp,
            "number" => Self::Number,
            "blockhash" => Self::BlockHash,
            "blobhash" => Self::BlobHash,
            "difficulty" => Self::Difficulty,
            "prevrandao" => Self::Prevrandao,
            "coinbase" => Self::CoinBase,
            "basefee" => Self::BaseFee,
            "blobbasefee" => Self::BlobBaseFee,
            "msize" => Self::MSize,

            "callcode" => Self::CallCode,
            "pc" => Self::Pc,
            "extcodecopy" => Self::ExtCodeCopy,
            "selfdestruct" => Self::SelfDestruct,

            "$zk_to_l1" => Self::ZkToL1,
            "$zk_code_source" => Self::ZkCodeSource,
            "$zk_precompile" => Self::ZkPrecompile,
            "$zk_meta" => Self::ZkMeta,
            "$zk_set_context_u128" => Self::ZkSetContextU128,
            "$zk_set_pubdata_price" => Self::ZkSetPubdataPrice,
            "$zk_increment_tx_counter" => Self::ZkIncrementTxCounter,
            "$zk_event_initialize" => Self::ZkEventInitialize,
            "$zk_event_write" => Self::ZkEventWrite,

            "$zk_mimic_call" => Self::ZkMimicCall,
            "$zk_system_mimic_call" => Self::ZkSystemMimicCall,
            "$zk_mimic_call_byref" => Self::ZkMimicCallByRef,
            "$zk_system_mimic_call_byref" => Self::ZkSystemMimicCallByRef,
            "$zk_raw_call" => Self::ZkRawCall,
            "$zk_raw_call_byref" => Self::ZkRawCallByRef,
            "$zk_system_call" => Self::ZkSystemCall,
            "$zk_system_call_byref" => Self::ZkSystemCallByRef,
            "$zk_static_raw_call" => Self::ZkStaticRawCall,
            "$zk_static_raw_call_byref" => Self::ZkStaticRawCallByRef,
            "$zk_static_system_call" => Self::ZkStaticSystemCall,
            "$zk_static_system_call_byref" => Self::ZkStaticSystemCallByRef,
            "$zk_delegate_raw_call" => Self::ZkDelegateRawCall,
            "$zk_delegate_raw_call_byref" => Self::ZkDelegateRawCallByRef,
            "$zk_delegate_system_call" => Self::ZkDelegateSystemCall,
            "$zk_delegate_system_call_byref" => Self::ZkDelegateSystemCallByRef,

            "$zk_load_calldata_into_active_ptr" => Self::ZkLoadCalldataIntoActivePtr,
            "$zk_load_returndata_into_active_ptr" => Self::ZkLoadReturndataIntoActivePtr,
            "$zk_ptr_add_into_active" => Self::ZkPtrAddIntoActive,
            "$zk_ptr_shrink_into_active" => Self::ZkPtrShrinkIntoActive,
            "$zk_ptr_pack_into_active" => Self::ZkPtrPackIntoActive,

            "$zk_multiplication_high" => Self::ZkMultiplicationHigh,

            "$zk_global_load" => Self::ZkGlobalLoad,
            "$zk_global_extra_abi_data" => Self::ZkGlobalExtraAbiData,
            "$zk_global_store" => Self::ZkGlobalStore,

            input => Self::UserDefined(input.to_owned()),
        }
    }
}
