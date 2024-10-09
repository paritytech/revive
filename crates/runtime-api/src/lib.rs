//! This crate vendors the [PolkaVM][0] C API and provides a LLVM module for interacting
//! with the `pallet-revive` runtime API.
//! At present, the contracts pallet requires blobs to export `call` and `deploy`,
//! and offers a bunch of [runtime API methods][1]. The provided [module] implements
//! those exports and imports.
//! [0]: [https://crates.io/crates/polkavm]
//! [1]: [https://docs.rs/pallet-contracts/26.0.0/pallet_contracts/api_doc/index.html]

pub mod calling_convention;
pub mod immutable_data;
pub mod polkavm_exports;
pub mod polkavm_imports;
