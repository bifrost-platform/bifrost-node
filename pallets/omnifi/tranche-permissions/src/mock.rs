//! Mock runtime for `pallet-tranche-permissions` unit tests and the
//! `impl_benchmark_test_suite!` harness.
//!
//! `T::Vaults` / `T::Products` are satisfied by permissive mock inspectors
//! rather than by wiring in the real `pallet-tranche-system` — this pallet only
//! ever reaches pallet-tranche-system through those two traits, and the
//! benchmarks characterise the real storage cost against the dev runtime, not
//! this mock.

use crate as pallet_tranche_permissions;

use frame_support::{construct_runtime, parameter_types, traits::Everything};
use pallet_tranche_system::{FlowVersion, ProductId, ProductInspect, VaultId, VaultInspect};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test {
		System: frame_system,
		TranchePermissions: pallet_tranche_permissions,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 256;
	pub const SS58Prefix: u8 = 42;
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
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
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

/// Every vault "belongs" to every product and every chain set checks out —
/// enough for the extrinsic bodies to run without pulling in pallet-tranche-system.
pub struct MockVaults;
impl VaultInspect for MockVaults {
	fn vault_belongs_to_product(_: ProductId, _: &VaultId) -> bool {
		true
	}
	fn vault_chains_belong_to_product(_: ProductId, _: &[u64]) -> bool {
		true
	}
	fn product_id_for_vault(_: &VaultId) -> Option<ProductId> {
		Some(0)
	}
}

/// Every product is a registered `Multichain` product (`single_chain_id` is
/// `None`), so `Role::TrancheInvestor` grants/revokes are never rejected as
/// `SingleChain`-only.
pub struct MockProducts;
impl ProductInspect for MockProducts {
	fn is_registered(_: ProductId) -> bool {
		true
	}
	fn single_chain_id(_: ProductId) -> Option<u64> {
		None
	}
	fn request_flow_version(_: ProductId) -> Option<FlowVersion> {
		Some(FlowVersion::V1)
	}
	fn settlement_flow_version(_: ProductId) -> Option<FlowVersion> {
		Some(FlowVersion::V1)
	}
}

impl pallet_tranche_permissions::Config for Test {
	type ProductAdminOrigin = frame_system::EnsureSigned<u64>;
	type Vaults = MockVaults;
	type Products = MockProducts;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
