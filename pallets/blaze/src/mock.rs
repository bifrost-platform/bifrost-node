use crate as pallet_blaze;
use bp_btc_relay::{
	traits::{PoolManager, SocketQueueManager, SocketVerifier},
	BoundedBitcoinAddress, MigrationSequence, UnboundedBytes,
};
use bp_core::{AccountId, Balance, BlockNumber};
use bp_staking::traits::Authorities;
use fp_account::{EthereumSignature, EthereumSigner};
use frame_support::{construct_runtime, parameter_types, traits::Everything};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	transaction_validity::TransactionValidityError,
	BuildStorage, DispatchError,
};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Blaze: pallet_blaze,
	}
);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 256;
	pub const SS58Prefix: u8 = 42;
	pub const FeeRateExpiration: u32 = 100;
	pub const ToleranceThreshold: u32 = 3;
	pub const ExistentialDeposit: u128 = 1;
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
pub struct MockAuthorities;
impl Authorities<AccountId> for MockAuthorities {
	fn is_authority(_: &AccountId) -> bool {
		true
	}

	fn majority() -> u32 {
		1
	}

	fn count() -> usize {
		1
	}
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

impl SocketVerifier<AccountId> for MockSocketQueue {
	fn verify_socket_message(_: &UnboundedBytes) -> Result<(), DispatchError> {
		Ok(())
	}
}

pub struct MockPoolManager;
impl PoolManager<AccountId> for MockPoolManager {
	fn get_bonded_descriptor(
		_: &BoundedBitcoinAddress,
	) -> Option<miniscript::Descriptor<miniscript::bitcoin::PublicKey>> {
		// Provide a valid descriptor for benchmarking
		use miniscript::{bitcoin::PublicKey, Descriptor};
		use std::str::FromStr;
		let pubkey_str = "02e6642fd69bd211f93f7f1f36ca51a26a5290eb2dd1b0d8279a87bb0d480c8443";
		let pubkey = PublicKey::from_str(pubkey_str).ok()?;
		Some(Descriptor::new_pk(pubkey))
	}

	fn get_refund_address(_: &AccountId) -> Option<BoundedBitcoinAddress> {
		None
	}

	fn get_vault_address(_: &AccountId) -> Option<BoundedBitcoinAddress> {
		None
	}

	fn get_system_vault(_: u32) -> Option<BoundedBitcoinAddress> {
		None
	}

	fn get_bitcoin_network() -> bp_btc_relay::Network {
		bp_btc_relay::Network::Bitcoin
	}

	fn get_bitcoin_chain_id() -> u32 {
		1
	}

	fn get_service_state() -> MigrationSequence {
		MigrationSequence::Normal
	}

	fn get_current_round() -> pallet_btc_registration_pool::PoolRound {
		0
	}

	fn add_migration_tx(_: H256) {}

	fn remove_migration_tx(_: H256) {}

	fn execute_migration_tx(_: H256) {}

	fn replace_authority(_: &AccountId, _: &AccountId) {}

	fn process_set_refunds() {}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_benchmark(_: &[AccountId], _: &AccountId) -> Result<(), DispatchError> {
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_service_state(_: MigrationSequence) -> Result<(), DispatchError> {
		Ok(())
	}
}

impl pallet_blaze::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Signature = EthereumSignature;
	type Signer = EthereumSigner;
	type Relayers = MockAuthorities;
	type SocketQueue = MockSocketQueue;
	type RegistrationPool = MockPoolManager;
	type FeeRateExpiration = FeeRateExpiration;
	type ToleranceThreshold = ToleranceThreshold;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
