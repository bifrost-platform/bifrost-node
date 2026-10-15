use crate as pallet_tranche_tx_registry_v3;
use frame_support::{derive_impl, ord_parameter_types};
use frame_system::{EnsureRoot, EnsureSignedBy};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

pub const RECORDER: u64 = 1;
pub const OTHER: u64 = 2;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Registry: pallet_tranche_tx_registry_v3,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

ord_parameter_types! {
	pub const Recorder: u64 = RECORDER;
}

impl pallet_tranche_tx_registry_v3::Config for Test {
	type GovernanceOrigin = EnsureRoot<u64>;
	type RecorderOrigin = EnsureSignedBy<Recorder, u64>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_tranche_tx_registry_v3::GenesisConfig::<Test>::default()
		.assimilate_storage(&mut storage)
		.unwrap();
	let mut ext = sp_io::TestExternalities::new(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
