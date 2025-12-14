use frame_support::{runtime, traits::FindAuthor, weights::constants::WEIGHT_REF_TIME_PER_SECOND};
use pallet_revive::AccountId32Mapper;
use polkadot_sdk::*;
use polkadot_sdk::{
    polkadot_sdk_frame::runtime::prelude::*,
    sp_runtime::{AccountId32, Perbill},
};

pub type Balance = u128;
pub type AccountId = pallet_revive::AccountId32Mapper<Runtime>;
pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub type Hash = <Runtime as frame_system::Config>::Hash;

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
        RuntimeTask,
        RuntimeViewFunction
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
    type BlockWeights = BlockWeights;
    type AccountId = AccountId32;
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
    pub BlockWeights: frame_system::limits::BlockWeights =
        frame_system::limits::BlockWeights::simple_max(
            Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
        );
}

#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
impl pallet_revive::Config for Runtime {
    type Time = Timestamp;
    type Currency = Balances;
    type DepositPerByte = DepositPerByte;
    type DepositPerItem = DepositPerItem;
    type AddressMapper = AccountId32Mapper<Self>;
    type RuntimeMemory = ConstU32<{ 512 * 1024 * 1024 }>;
    type PVFMemory = ConstU32<{ 1024 * 1024 * 1024 }>;
    type UnsafeUnstableInterface = UnstableInterface;
    type UploadOrigin = EnsureSigned<Self::AccountId>;
    type InstantiateOrigin = EnsureSigned<Self::AccountId>;
    type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
    type ChainId = ConstU64<420_420_420>;
    type FindAuthor = Self;
    type NativeToEthRatio = ConstU32<{ crate::ETH_RATIO as u32 }>;
}

impl FindAuthor<<Runtime as frame_system::Config>::AccountId> for Runtime {
    fn find_author<'a, I>(_digests: I) -> Option<<Runtime as frame_system::Config>::AccountId>
    where
        I: 'a + IntoIterator<Item = (frame_support::ConsensusEngineId, &'a [u8])>,
    {
        Some(
            [[0xff; 20].as_slice(), [0xee; 12].as_slice()]
                .concat()
                .as_slice()
                .try_into()
                .unwrap(),
        )
    }
}
