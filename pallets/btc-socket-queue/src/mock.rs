use crate as pallet_btc_socket_queue;

use bp_btc_relay::{
	blaze::{ScoredUtxo, SelectionStrategy, UtxoInfoWithSize},
	traits::{BlazeManager, PoolManager},
	BoundedBitcoinAddress, MigrationSequence, UnboundedBytes,
};
use bp_core::{AccountId, Balance, BlockNumber};
use bp_staking::traits::Authorities;
use fp_account::{EthereumSignature, EthereumSigner};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{Everything, SortedMembers},
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_evm::FeeCalculator;
use sp_core::{H256, U256};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, DispatchError,
};

type Block = frame_system::mocking::MockBlock<Test>;

pub struct MockPrecompiles;
impl pallet_evm::PrecompileSet for MockPrecompiles {
	fn execute(
		&self,
		_handle: &mut impl pallet_evm::PrecompileHandle,
	) -> Option<pallet_evm::PrecompileResult> {
		None
	}
	fn is_precompile(&self, _address: sp_core::H160, _gas: u64) -> pallet_evm::IsPrecompileResult {
		pallet_evm::IsPrecompileResult::Answer { is_precompile: false, extra_cost: 0 }
	}
}

construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Timestamp: pallet_timestamp,
		EVM: pallet_evm,
		BtcSocketQueue: pallet_btc_socket_queue,
	}
);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 256;
	pub const SS58Prefix: u8 = 42;
	pub const ExistentialDeposit: u128 = 1;
	pub const DefaultMaxFeeRate: u64 = 1000;
	pub const MinimumPeriod: u64 = 6000;
	pub PrecompilesValue: MockPrecompiles = MockPrecompiles;
	pub WeightPerGas: frame_support::weights::Weight = frame_support::weights::Weight::from_parts(1, 0);
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

pub struct MockFeeCalculator;
impl FeeCalculator for MockFeeCalculator {
	fn min_gas_price() -> (U256, frame_support::weights::Weight) {
		(U256::from(1000000000u64), frame_support::weights::Weight::zero())
	}
}

impl pallet_evm::Config for Test {
	type FeeCalculator = MockFeeCalculator;
	type GasWeightMapping = pallet_evm::FixedGasWeightMapping<Self>;
	type WeightPerGas = WeightPerGas;
	type BlockHashMapping = pallet_evm::SubstrateBlockHashMapping<Self>;
	type CallOrigin = pallet_evm::EnsureAddressRoot<AccountId>;
	type WithdrawOrigin = pallet_evm::EnsureAddressNever<AccountId>;
	type AddressMapping = pallet_evm::IdentityAddressMapping;
	type Currency = Balances;
	type RuntimeEvent = RuntimeEvent;
	type PrecompilesType = MockPrecompiles;
	type PrecompilesValue = PrecompilesValue;
	type ChainId = ();
	type BlockGasLimit = ();
	type Runner = pallet_evm::runner::stack::Runner<Self>;
	type OnChargeTransaction = ();
	type OnCreate = ();
	type FindAuthor = ();
	type GasLimitPovSizeRatio = ();
	type SuicideQuickClearLimit = ();
	type Timestamp = Timestamp;
	type WeightInfo = ();
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
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

pub struct MockRelayers;
impl Authorities<AccountId> for MockRelayers {
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

pub struct MockPoolManager;
impl PoolManager<AccountId> for MockPoolManager {
	fn get_bonded_descriptor(
		_: &BoundedBitcoinAddress,
	) -> Option<miniscript::Descriptor<miniscript::bitcoin::PublicKey>> {
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
		bp_btc_relay::Network::Regtest
	}

	fn get_bitcoin_chain_id() -> u32 {
		10002
	}

	fn get_service_state() -> MigrationSequence {
		MigrationSequence::Normal
	}

	fn get_current_round() -> u32 {
		1
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

pub struct MockBlazeManager;
impl BlazeManager<Test> for MockBlazeManager {
	fn is_activated() -> bool {
		true
	}

	fn get_utxos() -> Vec<UtxoInfoWithSize> {
		vec![]
	}

	fn clear_utxos() {}

	fn lock_utxos(_: &H256, _: &Vec<UtxoInfoWithSize>) -> Result<(), DispatchError> {
		Ok(())
	}

	fn unlock_utxos(_: &H256) -> Result<(), DispatchError> {
		Ok(())
	}

	fn extract_utxos_from_psbt(
		_: &bp_btc_relay::Psbt,
	) -> Result<Vec<UtxoInfoWithSize>, DispatchError> {
		Ok(vec![])
	}

	fn get_outbound_pool() -> Vec<UnboundedBytes> {
		vec![]
	}

	fn clear_outbound_pool(_: Vec<UnboundedBytes>) {}

	fn try_fee_rate_finalization(_: BlockNumberFor<Test>) -> Option<(u64, u64)> {
		None
	}

	fn clear_fee_rates() {}

	fn select_coins(
		_: Vec<ScoredUtxo>,
		_: u64,
		_: u64,
		_: u64,
		_: usize,
		_: u64,
	) -> Option<(Vec<UtxoInfoWithSize>, SelectionStrategy)> {
		None
	}

	fn handle_tolerance_counter(_: bool) {}

	fn ensure_activation(_: bool) -> Result<(), DispatchError> {
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_activation(_: bool) -> Result<(), DispatchError> {
		Ok(())
	}
}

impl pallet_btc_socket_queue::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Signature = EthereumSignature;
	type Signer = EthereumSigner;
	type Executives = MockExecutives;
	type Relayers = MockRelayers;
	type RegistrationPool = MockPoolManager;
	type Blaze = MockBlazeManager;
	type WeightInfo = ();
	type DefaultMaxFeeRate = DefaultMaxFeeRate;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
