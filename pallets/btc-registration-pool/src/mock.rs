use crate as pallet_btc_registration_pool;
use bp_btc_relay::{traits::SocketQueueManager, Network};
use bp_core::{AccountId, Balance, BlockNumber};
use fp_account::{EthereumSignature, EthereumSigner};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{Everything, SortedMembers},
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup, Lazy, Verify},
	transaction_validity::TransactionValidityError,
	BuildStorage, Percent,
};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		BtcRegistrationPool: pallet_btc_registration_pool,
	}
);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 256;
	pub const SS58Prefix: u8 = 42;
	pub const ExistentialDeposit: u128 = 1;
	pub const DefaultMultiSigRatio: Percent = Percent::from_percent(67);
	pub const BitcoinChainId: u32 = 10002;
	pub const BitcoinNetwork: Network = Network::Regtest;
}

impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeTask = RuntimeTask;
	type Nonce = u64;
	type Block = Block;
	type RuntimeCall = RuntimeCall;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type BlockWeights = ();
	type BlockLength = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
}

impl pallet_balances::Config for Test {
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
}

// Mock implementations for required traits
pub struct MockExecutives;
impl SortedMembers<AccountId> for MockExecutives {
	fn sorted_members() -> Vec<AccountId> {
		vec![AccountId::from([1u8; 20]), AccountId::from([2u8; 20]), AccountId::from([3u8; 20])]
	}

	fn count() -> usize {
		3
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add(_: &AccountId) {}
}

pub struct MockSocketQueue;
impl SocketQueueManager<AccountId> for MockSocketQueue {
	fn is_ready_for_migrate() -> bool {
		true
	}

	fn verify_authority(_: &AccountId) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	fn replace_authority(_: &AccountId, _: &AccountId) {}

	fn get_max_fee_rate() -> u64 {
		u64::MAX
	}
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Debug, Clone, PartialEq, Eq)]
pub struct MockEthereumSignature(EthereumSignature);
impl Verify for MockEthereumSignature {
	type Signer = EthereumSigner;
	fn verify<L: Lazy<[u8]>>(&self, _: L, _: &AccountId) -> bool {
		true
	}
}

impl pallet_btc_registration_pool::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Signature = MockEthereumSignature;
	type Signer = EthereumSigner;
	type Executives = MockExecutives;
	type SocketQueue = MockSocketQueue;
	type DefaultMultiSigRatio = DefaultMultiSigRatio;
	type BitcoinChainId = BitcoinChainId;
	type BitcoinNetwork = BitcoinNetwork;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
