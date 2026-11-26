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

/// Every chain but `UNREGISTERED_CHAIN` is registered, with 32-byte tx hashes unless a
/// benchmark seeded it otherwise (the benchmarks record max-length hashes).
pub const UNREGISTERED_CHAIN: u64 = u64::MAX;
pub struct MockChains;

pub fn mock_chain_key(chain_id: u64) -> Vec<u8> {
	parity_scale_codec::Encode::encode(&(b"mock-chain", chain_id))
}

impl pallet_tranche_tx_registry_v3::ChainInspect for MockChains {
	fn chain_info(chain_id: u64) -> Option<pallet_tranche_tx_registry_v3::ChainInfo> {
		(chain_id != UNREGISTERED_CHAIN).then(|| {
			frame_support::storage::unhashed::get(&mock_chain_key(chain_id)).unwrap_or(
				pallet_tranche_tx_registry_v3::ChainInfo { address_len: 20, tx_hash_len: 32 },
			)
		})
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl pallet_tranche_tx_registry_v3::BenchmarkHelper for MockChains {
	fn seed_recorder() {}
	fn seed_chain(chain_id: u64, info: pallet_tranche_tx_registry_v3::ChainInfo) {
		frame_support::storage::unhashed::put(&mock_chain_key(chain_id), &info);
	}
}

impl pallet_tranche_tx_registry_v3::Config for Test {
	type GovernanceOrigin = EnsureRoot<u64>;
	type RecorderOrigin = EnsureSignedBy<Recorder, u64>;
	type Chains = MockChains;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = MockChains;
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
