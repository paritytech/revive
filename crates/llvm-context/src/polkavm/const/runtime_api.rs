//! Runtime API import and export symbols.

pub mod exports {
    /// The contract deploy export.
    pub static CALL: &str = "call";

    /// The contract call export.
    pub static DEPLOY: &str = "deploy";

    /// All exported symbols.
    /// Useful for configuring common attributes and linkage.
    pub static EXPORTS: [&str; 2] = [CALL, DEPLOY];
}

pub mod imports {
    pub static SBRK: &str = "__sbrk_internal";

    pub static MEMORY_SIZE: &str = "__msize";

    pub static ADDRESS: &str = "address";

    pub static BALANCE: &str = "balance";

    pub static BALANCE_OF: &str = "balance_of";

    pub static BLOCK_NUMBER: &str = "block_number";

    pub static CHAIN_ID: &str = "chain_id";

    pub static CALL: &str = "call";

    pub static DELEGATE_CALL: &str = "delegate_call";

    pub static CALLER: &str = "caller";

    pub static CODE_SIZE: &str = "code_size";

    pub static CODE_HASH: &str = "code_hash";

    pub static DEPOSIT_EVENT: &str = "deposit_event";

    pub static GET_IMMUTABLE_DATA: &str = "get_immutable_data";

    pub static GET_STORAGE: &str = "get_storage";

    pub static HASH_KECCAK_256: &str = "hash_keccak_256";

    pub static INPUT: &str = "input";

    pub static INSTANTIATE: &str = "instantiate";

    pub static NOW: &str = "now";

    pub static RETURN: &str = "seal_return";

    pub static RETURNDATACOPY: &str = "return_data_copy";

    pub static RETURNDATASIZE: &str = "return_data_size";

    pub static SET_STORAGE: &str = "set_storage";

    pub static SET_IMMUTABLE_DATA: &str = "set_immutable_data";

    pub static VALUE_TRANSFERRED: &str = "value_transferred";

    /// All imported runtime API symbols.
    /// Useful for configuring common attributes and linkage.
    pub static IMPORTS: [&str; 25] = [
        SBRK,
        MEMORY_SIZE,
        ADDRESS,
        BALANCE,
        BALANCE_OF,
        BLOCK_NUMBER,
        CALL,
        DELEGATE_CALL,
        CALLER,
        CHAIN_ID,
        CODE_SIZE,
        CODE_HASH,
        DEPOSIT_EVENT,
        GET_IMMUTABLE_DATA,
        GET_STORAGE,
        HASH_KECCAK_256,
        INPUT,
        INSTANTIATE,
        NOW,
        RETURN,
        RETURNDATACOPY,
        RETURNDATASIZE,
        SET_IMMUTABLE_DATA,
        SET_STORAGE,
        VALUE_TRANSFERRED,
    ];
}
