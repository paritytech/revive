use frame_support::runtime;

use polkadot_sdk::*;
use polkadot_sdk::{
    polkadot_sdk_frame::{log, runtime::prelude::*},
    sp_runtime::{AccountId32, Perbill},
};

pub type Balance = u128;
pub type AccountId = AccountId32;
pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub type Hash = <Runtime as frame_system::Config>::Hash;
pub type EventRecord =
    frame_system::EventRecord<<Runtime as frame_system::Config>::RuntimeEvent, Hash>;

#[runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask
    )]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;

    #[runtime::pallet_index(2)]
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(3)]
    pub type Contracts = pallet_revive;
}

#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
    type AccountId = AccountId;
    type AccountData = pallet_balances::AccountData<<Runtime as pallet_balances::Config>::Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
    type AccountStore = System;
    type Balance = Balance;
    type ExistentialDeposit = ConstU128<1_000>;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Runtime {}

parameter_types! {
    pub const UnstableInterface: bool = true;
    pub const DepositPerByte: Balance = 1;
    pub const DepositPerItem: Balance = 2;
    pub const CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(0);
}

#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
impl pallet_revive::Config for Runtime {
    type Time = Timestamp;
    type Currency = Balances;
    type CallFilter = ();
    type ChainExtension = ();
    type DepositPerByte = DepositPerByte;
    type DepositPerItem = DepositPerItem;
    type AddressGenerator = pallet_revive::DefaultAddressGenerator;
    type UnsafeUnstableInterface = UnstableInterface;
    type UploadOrigin = EnsureSigned<AccountId>;
    type InstantiateOrigin = EnsureSigned<AccountId>;
    type Migrations = ();
    type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
    type Debug = ();
}
