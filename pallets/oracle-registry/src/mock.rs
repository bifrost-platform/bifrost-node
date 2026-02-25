use crate as pallet_oracle_registry;
use bp_core::{AccountId, Balance, BlockNumber};
use frame_support::{construct_runtime, parameter_types, traits::Everything};
use pallet_evm::FeeCalculator;
use sp_core::{H256, U256};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
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
		OracleRegistry: pallet_oracle_registry,
	}
);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 256;
	pub const SS58Prefix: u8 = 42;
	pub const ExistentialDeposit: u128 = 1;
	pub const MinimumPeriod: u64 = 6000;
	pub PrecompilesValue: MockPrecompiles = MockPrecompiles;
	pub WeightPerGas: frame_support::weights::Weight =
		frame_support::weights::Weight::from_parts(1, 0);
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
	type ExtensionsWeightInfo = ();
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
	type DoneSlashHandler = ();
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

pub struct MockFeeCalculator;
impl FeeCalculator for MockFeeCalculator {
	fn min_gas_price() -> (U256, frame_support::weights::Weight) {
		(U256::from(1_000_000_000u64), frame_support::weights::Weight::zero())
	}
}

impl pallet_evm::Config for Test {
	type AccountProvider = pallet_evm::FrameSystemAccountProvider<Self>;
	type FeeCalculator = MockFeeCalculator;
	type GasWeightMapping = pallet_evm::FixedGasWeightMapping<Self>;
	type WeightPerGas = WeightPerGas;
	type BlockHashMapping = pallet_evm::SubstrateBlockHashMapping<Self>;
	type CallOrigin = pallet_evm::EnsureAddressRoot<AccountId>;
	type WithdrawOrigin = pallet_evm::EnsureAddressNever<AccountId>;
	type AddressMapping = pallet_evm::IdentityAddressMapping;
	type Currency = Balances;
	type PrecompilesType = MockPrecompiles;
	type PrecompilesValue = PrecompilesValue;
	type ChainId = ();
	type BlockGasLimit = ();
	type Runner = pallet_evm::runner::stack::Runner<Self>;
	type OnChargeTransaction = ();
	type OnCreate = ();
	type FindAuthor = ();
	type GasLimitPovSizeRatio = ();
	type GasLimitStorageGrowthRatio = frame_support::traits::ConstU64<2>;
	type CreateOriginFilter = ();
	type CreateInnerOriginFilter = ();
	type FeelessCallFilter = ();
	type Timestamp = Timestamp;
	type WeightInfo = ();
}

impl pallet_oracle_registry::Config for Test {
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
