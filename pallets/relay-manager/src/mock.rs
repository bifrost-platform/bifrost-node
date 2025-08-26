use crate as pallet_relay_manager;
use crate::{IdentificationTuple, UnresponsivenessOffence};

use bp_btc_relay::{
	traits::{PoolManager, SocketQueueManager, SocketVerifier},
	BoundedBitcoinAddress, Descriptor, MigrationSequence, PublicKey, UnboundedBytes,
};
use bp_core::{AccountId, Balance, BlockNumber};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{Everything, ValidatorSet, ValidatorSetWithIdentification},
};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, Convert, IdentityLookup},
	transaction_validity::TransactionValidityError,
	BuildStorage, DispatchError, Perbill,
};
use sp_staking::{
	offence::{OffenceError, ReportOffence},
	SessionIndex,
};

construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		RelayManager: pallet_relay_manager,
	}
);

type Block = frame_system::mocking::MockBlock<Test>;

parameter_types! {
	pub const BlockHashCount: BlockNumber = 256;
	pub const SS58Prefix: u8 = 42;
	pub const ExistentialDeposit: u128 = 1;
	pub const StorageCacheLifetimeInRounds: u32 = 100;
	pub const IsHeartbeatOffenceActive: bool = true;
	pub const DefaultHeartbeatSlashFraction: Perbill = Perbill::from_percent(10);
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

	#[cfg(feature = "runtime-benchmarks")]
	fn set_max_fee_rate(_: u64) {}
}

impl SocketVerifier<AccountId> for MockSocketQueue {
	fn verify_socket_message(_: &UnboundedBytes) -> Result<(), DispatchError> {
		Ok(())
	}
}

pub struct MockPoolManager;
impl PoolManager<AccountId> for MockPoolManager {
	fn get_bonded_descriptor(_: &BoundedBitcoinAddress) -> Option<Descriptor<PublicKey>> {
		None
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

// Create a simple identity converter for AccountId
pub struct IdentityConverter;
impl Convert<AccountId, Option<AccountId>> for IdentityConverter {
	fn convert(account: AccountId) -> Option<AccountId> {
		Some(account)
	}
}

pub struct MockValidatorSet;
impl ValidatorSet<AccountId> for MockValidatorSet {
	type ValidatorId = AccountId;
	type ValidatorIdOf = IdentityConverter;

	fn session_index() -> SessionIndex {
		0
	}

	fn validators() -> Vec<Self::ValidatorId> {
		// Create some mock AccountIds using H256 conversion
		vec![AccountId::from([1u8; 20]), AccountId::from([2u8; 20]), AccountId::from([3u8; 20])]
	}
}

impl ValidatorSetWithIdentification<AccountId> for MockValidatorSet {
	type Identification = AccountId;
	type IdentificationOf = IdentityConverter;
}

pub struct MockReportUnresponsiveness;
impl
	ReportOffence<
		AccountId,
		IdentificationTuple<Test>,
		UnresponsivenessOffence<IdentificationTuple<Test>, Test>,
	> for MockReportUnresponsiveness
{
	fn report_offence(
		_reporters: Vec<AccountId>,
		_offence: UnresponsivenessOffence<IdentificationTuple<Test>, Test>,
	) -> Result<(), OffenceError> {
		Ok(())
	}

	fn is_known_offence(
		_offenders: &[IdentificationTuple<Test>],
		_time_slot: &SessionIndex,
	) -> bool {
		false
	}
}

impl pallet_relay_manager::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type SocketQueue = MockSocketQueue;
	type RegistrationPool = MockPoolManager;
	type ValidatorSet = MockValidatorSet;
	type ReportUnresponsiveness = MockReportUnresponsiveness;
	type StorageCacheLifetimeInRounds = StorageCacheLifetimeInRounds;
	type IsHeartbeatOffenceActive = IsHeartbeatOffenceActive;
	type DefaultHeartbeatSlashFraction = DefaultHeartbeatSlashFraction;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
