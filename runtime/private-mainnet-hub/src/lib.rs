//! Runtime for the private Hub chain of OmniFi mainnet.
//!
//! Hosts the OmniFi tranche-* stack plus what this chain needs to operate it
//! (see docs/confidential-layer/provisioning.md): validator/relayer management
//! (`PermissionedAuthority`, `RelayManager`, offences, im-online), CCCP relaying
//! (`CCCPRelayQueue`, `OracleRegistry`), and emergency controls (`SafeMode`, `TxPause`). Compared to
//! the public Bifrost runtimes it replaces DPoS staking (`BfcStaking`/`BfcOffences`) with a
//! root-managed authority set, and drops the BTC bridge pallets, ERC20 gas payment
//! (`evm-tx-payment`), on-chain governance
//! (council/democracy/treasury), identity, and scheduler. Administration is done
//! through `Sudo`.

// Build both the Native Rust binary and the WASM binary.
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

pub use bifrost_private_mainnet_hub_constants::{
	currency::{GWEI, UNITS as BFC, *},
	fee::*,
	time::*,
};

pub use bp_core::{AccountId, Address, Balance, BlockNumber, Hash, Header, Nonce, Signature};
use fp_account::{EthereumSignature, EthereumSigner};
use fp_rpc::TransactionStatus;
use fp_rpc_txpool::TxPoolResponse;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, ConstBool, ConstU64, OpaqueMetadata, H160, H256, U256};
use sp_genesis_builder::PresetId;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::{
	generic, impl_opaque_keys,
	traits::{
		BlakeTwo256, Block as BlockT, ConvertInto, ConvertToValue, DispatchInfoOf, Dispatchable,
		IdentityLookup, NumberFor, OpaqueKeys, PostDispatchInfoOf, UniqueSaturatedInto,
	},
	transaction_validity::{
		TransactionPriority, TransactionSource, TransactionValidity, TransactionValidityError,
	},
	ApplyExtrinsicResult,
};
pub use sp_runtime::{traits, ExtrinsicInclusionMode, Perbill, Percent, Permill};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use parity_scale_codec::{Decode, Encode};

pub use pallet_balances::Call as BalancesCall;
use pallet_ethereum::{
	Call::transact, EthereumBlockHashMapping, PostLogContent, Transaction as EthereumTransaction,
};
use pallet_evm::{
	Account as EVMAccount, EVMCurrencyAdapter, EnsureAddressNever, EnsureAddressRoot,
	FeeCalculator, IdentityAddressMapping, Runner,
};
use pallet_grandpa::{
	fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList,
};
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
pub use pallet_timestamp::Call as TimestampCall;
#[allow(deprecated)]
use pallet_transaction_payment::CurrencyAdapter;

pub use frame_support::{
	derive_impl,
	dispatch::{DispatchClass, GetDispatchInfo},
	genesis_builder_helper::{build_state, get_preset},
	pallet_prelude::Get,
	parameter_types,
	traits::{ConstU128, ConstU32, ConstU8, Contains, FindAuthor, InsideBoth, OnFinalize},
	weights::{
		constants::{
			BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND,
		},
		ConstantMultiplier, IdentityFee, Weight,
	},
	ConsensusEngineId, StorageValue,
};
use frame_system::{EnsureRoot, EnsureRootWithSuccess};

mod precompiles;
pub use precompiles::BifrostPrecompiles;

pub type Precompiles = BifrostPrecompiles<Runtime>;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// The `TransactionExtension` to the basic transaction logic.
pub type TxExtension = (
	bifrost_common_runtime::extensions::CheckBlockedOrigin<Runtime, RuntimeCall>,
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
	frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
	frame_system::WeightReclaim<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	fp_self_contained::UncheckedExtrinsic<Address, RuntimeCall, Signature, TxExtension>;

/// Every precompile address in this runtime's precompile set, as `H160`.
pub struct PrecompileAddresses;
impl frame_support::traits::Get<Vec<H160>> for PrecompileAddresses {
	fn get() -> Vec<H160> {
		Precompiles::used_addresses().map(Into::into).collect()
	}
}

/// All migrations executed on runtime upgrade as a nested tuple of types implementing
/// `OnRuntimeUpgrade`.
type SingleBlockMigrations =
	(bifrost_common_runtime::migrations::EnsurePrecompileCode<Runtime, PrecompileAddresses>,);

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
>;

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
	use super::*;
	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

	pub type Block = generic::Block<Header, UncheckedExtrinsic>;

	impl_opaque_keys! {
		pub struct SessionKeys {
			pub aura: Aura,
			pub grandpa: Grandpa,
			pub im_online: ImOnline,
		}
	}
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	// The identifier for the different Substrate runtimes.
	spec_name: alloc::borrow::Cow::Borrowed("thebifrost-private-mainnet-hub"),
	// The name of the implementation of the spec.
	impl_name: alloc::borrow::Cow::Borrowed("bifrost-private-mainnet-hub"),
	// The version of the authorship interface.
	authoring_version: 1,
	// The version of the runtime spec, encoded as semver `MAJOR_MINOR_PATCH` with three digits
	// per minor/patch: `1_000_000` = v1.0.0, `1_002_003` = v1.2.3.
	spec_version: 1_000_000,
	// The version of the implementation of the spec.
	impl_version: 1,
	// A list of supported runtime APIs along with their versions.
	apis: RUNTIME_API_VERSIONS,
	// The version of the interface for handling transactions.
	transaction_version: 1,
	// The version of the interface for handling state transitions.
	system_version: 1,
};

/// Maximum weight per block.
/// We allow for 0.5 seconds of compute with a 3 second average block time, with maximum proof size.
const MAXIMUM_BLOCK_WEIGHT: Weight =
	Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_div(2), u64::MAX);

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used
/// by  Operational  extrinsics.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
	pub const BlockHashCount: BlockNumber = 256;
	pub BlockWeights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights
		::with_sensible_defaults(MAXIMUM_BLOCK_WEIGHT, NORMAL_DISPATCH_RATIO);
	pub BlockLength: frame_system::limits::BlockLength = frame_system::limits::BlockLength
		::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

/// The System pallet defines the core data types used in a Substrate runtime
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = InsideBoth<InsideBoth<SafeMode, TxPause>, NotSelfManagedRelayers>;
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = BlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = BlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The lookup mechanism to get the account ID from whatever is passed in dispatchers.
	type Lookup = IdentityLookup<AccountId>;
	/// The index type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// Provides information about the pallet setup in the runtime.
	type PalletInfo = PalletInfo;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	/// The maximum number of consumers allowed on a single account.
	type MaxConsumers = ConstU32<16>;
	/// Single block migrations
	type SingleBlockMigrations = SingleBlockMigrations;
}

/// Relayer replacement is root-only on this chain (`PermissionedAuthority::set_relayer` /
/// `cancel_relayer_set`), so `RelayManager`'s own controller-signed variants are disabled.
pub struct NotSelfManagedRelayers;
impl Contains<RuntimeCall> for NotSelfManagedRelayers {
	fn contains(call: &RuntimeCall) -> bool {
		!matches!(
			call,
			RuntimeCall::RelayManager(
				pallet_relay_manager::Call::set_relayer { .. }
					| pallet_relay_manager::Call::cancel_relayer_set { .. }
			)
		)
	}
}

/// Calls that can bypass the safe-mode pallet.
pub struct SafeModeWhitelistedCalls;
impl Contains<RuntimeCall> for SafeModeWhitelistedCalls {
	fn contains(call: &RuntimeCall) -> bool {
		match call {
			RuntimeCall::System(_)
			| RuntimeCall::Sudo(_)
			| RuntimeCall::Timestamp(_)
			| RuntimeCall::SafeMode(_)
			| RuntimeCall::TxPause(_)
			| RuntimeCall::ImOnline(pallet_im_online::Call::heartbeat { .. })
			| RuntimeCall::RelayManager(pallet_relay_manager::Call::heartbeat { .. })
			| RuntimeCall::RelayManager(pallet_relay_manager::Call::heartbeat_v2 { .. }) => true,
			_ => false,
		}
	}
}

/// Calls that can never be paused by `TxPause`. `Sudo` is the only `Root` origin on this
/// chain (no council/democracy), and unpausing itself requires `Root` — pausing any `Sudo`
/// call would permanently remove the chain's admin.
pub struct TxPauseWhitelistedCalls;
impl Contains<pallet_tx_pause::RuntimeCallNameOf<Runtime>> for TxPauseWhitelistedCalls {
	fn contains(full_name: &pallet_tx_pause::RuntimeCallNameOf<Runtime>) -> bool {
		full_name.0.as_slice() == b"Sudo"
	}
}

impl pallet_tx_pause::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PauseOrigin = EnsureRoot<AccountId>;
	type UnpauseOrigin = EnsureRoot<AccountId>;
	type WhitelistedCalls = TxPauseWhitelistedCalls;
	type MaxNameLen = ConstU32<256>;
	type WeightInfo = pallet_tx_pause::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const EnterDuration: BlockNumber = 12 * HOURS;
	pub const EnterDepositAmount: Option<Balance> = None;
	pub const ExtendDuration: BlockNumber = 1 * HOURS;
	pub const ExtendDepositAmount: Option<Balance> = None;
	pub const ReleaseDelay: u32 = 1 * HOURS;
}

/// Same as mainnet: safe mode can only be entered/extended/exited by `Root`
/// (no permissionless deposits).
impl pallet_safe_mode::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	type WhitelistedCalls = SafeModeWhitelistedCalls;
	type EnterDuration = EnterDuration;
	type EnterDepositAmount = EnterDepositAmount;
	type ExtendDuration = ExtendDuration;
	type ExtendDepositAmount = ExtendDepositAmount;
	type ForceEnterOrigin = EnsureRootWithSuccess<AccountId, EnterDuration>;
	type ForceExtendOrigin = EnsureRootWithSuccess<AccountId, ExtendDuration>;
	type ForceExitOrigin = EnsureRoot<AccountId>;
	type ForceDepositOrigin = EnsureRoot<AccountId>;
	type ReleaseDelay = ReleaseDelay;
	type Notify = ();
	type WeightInfo = pallet_safe_mode::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	/// The maximum number of validators. Shared by Aura, GRANDPA and `PermissionedAuthority`
	/// so the block producers, the finality voters and the CCCP relayer set can never diverge.
	pub const MaxAuthorities: u32 = 10;
}

/// Provides the Aura block production engine.
impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = MaxAuthorities;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

/// Provides the GRANDPA block finality gadget.
impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
	type WeightInfo = ();
	type MaxAuthorities = MaxAuthorities;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<0>;
}

parameter_types! {
	pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

/// A timestamp: milliseconds since the unix epoch.
impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = pallet_timestamp::weights::SubstrateWeight<Runtime>;
}

/// Provides functionality for handling accounts and balances.
impl pallet_balances::Config for Runtime {
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ConstU32<50>;
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU128<0>;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type FreezeIdentifier = ();
	type MaxFreezes = ConstU32<0>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

parameter_types! {
	pub const TransactionByteFee: Balance = TRANSACTION_BYTE_FEE;
}

/// Provides the basic logic needed to pay the absolute minimum amount needed for a transaction to
/// be included. There is no treasury on this chain, so fees are burned (the `()` handler drops
/// the imbalance); the gas token is re-issued by `Sudo` through `BfcUtility::mint_native`.
#[allow(deprecated)]
impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = CurrencyAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = ();
	type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

/// The Sudo module allows for a single account (called the "sudo key")
/// to execute dispatchable functions that require a `Root` call
/// or designate a new account to replace them as the sudo key.
impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const SessionPeriod: u32 = 15 * MINUTES; // 300 blocks
	pub const Offset: u32 = 0;
}

/// The Session module allows validators to manage their session keys, provides a function for
/// changing the session length, and handles session rotation.
impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = ConvertInto;
	type ShouldEndSession = pallet_session::PeriodicSessions<SessionPeriod, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<SessionPeriod, Offset>;
	type SessionManager = PermissionedAuthority;
	type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = opaque::SessionKeys;
	type DisablingStrategy = ();
	type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
	type Currency = Balances;
	type KeyDeposit = ConstU128<0>;
}

parameter_types! {
	pub const NoFullIdentification: Option<()> = Some(());
}

/// Authorities carry no stake, so there is nothing to identify beyond the account itself.
impl pallet_session::historical::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type FullIdentification = ();
	type FullIdentificationOf = ConvertToValue<NoFullIdentification>;
}

parameter_types! {
	pub const ImOnlineUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
	pub const MaxKeys: u32 = 10_000;
	pub const MaxPeerInHeartbeats: u32 = 10_000;
	pub const DefaultSlashFraction: Perbill = Perbill::from_parts(1000000);
}

impl<LocalCall> frame_system::offchain::CreateBare<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
		Self::Extrinsic::new_bare(call)
	}
}

impl<C> frame_system::offchain::CreateTransactionBase<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type RuntimeCall = RuntimeCall;
}

/// The module that manages validator livenesses.
impl pallet_im_online::Config for Runtime {
	type AuthorityId = ImOnlineId;
	type RuntimeEvent = RuntimeEvent;
	type NextSessionRotation = pallet_session::PeriodicSessions<SessionPeriod, Offset>;
	type ValidatorSet = Historical;
	type ReportUnresponsiveness = Offences;
	type UnsignedPriority = ImOnlineUnsignedPriority;
	type WeightInfo = pallet_im_online::weights::SubstrateWeight<Runtime>;
	type MaxKeys = MaxKeys;
	type MaxPeerInHeartbeats = MaxPeerInHeartbeats;
	type DefaultSlashFraction = DefaultSlashFraction;
}

/// The module that manages validator offences. Offences are only reported as
/// `PermissionedAuthority::OffenceReported` events — nothing is slashed or removed.
impl pallet_offences::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type IdentificationTuple = pallet_session::historical::IdentificationTuple<Self>;
	type OnOffenceHandler = PermissionedAuthority;
}

/// The Authorship module tracks the current author of the block. Notifying `ImOnline` makes
/// authoring a block count as being online, so a validator that is producing blocks is never
/// reported unresponsive just because its heartbeat didn't land.
impl pallet_authorship::Config for Runtime {
	type EventHandler = ImOnline;
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
}

parameter_types! {
	pub const StorageCacheLifetimeInRounds: u32 = 64u32;
	pub const IsHeartbeatOffenceActive: bool = false;
	pub const DefaultHeartbeatSlashFraction: Perbill = Perbill::from_parts(2000000);
}

/// A module that manages registered relayers for cross chain interoperability.
/// No BTC bridge runs on this chain: the BTC hooks and the relay executives (the BTC
/// multisig members) are `()` (only `replace_authority`/`replace_member` are ever called
/// on them, which become no-ops).
impl pallet_relay_manager::Config for Runtime {
	type Blaze = ();
	type SocketQueue = ();
	type RegistrationPool = ();
	type RelayExecutives = ();
	type RelayQueue = CCCPRelayQueue;
	type ValidatorSet = Historical;
	type ReportUnresponsiveness = Offences;
	type StorageCacheLifetimeInRounds = StorageCacheLifetimeInRounds;
	type IsHeartbeatOffenceActive = IsHeartbeatOffenceActive;
	type DefaultHeartbeatSlashFraction = DefaultHeartbeatSlashFraction;
	type WeightInfo = pallet_relay_manager::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	/// Maximum byte size of a single CCCP message accepted by `CCCPRelayQueue`. A runtime
	/// `storage` parameter (same 2 KiB default as the public chains' `BtcSocketQueue`
	/// setting), so `Sudo` can change it without a runtime upgrade:
	///
	/// - key:   `twox_128(":MaxCccpMessageBytes:")` = `0x86e6336b46c0f69167a6b85c7a80a15d`
	/// - value: SCALE-encoded `u32` (little-endian), e.g. 4 KiB = `0x00100000`
	///
	/// `sudo(system.setStorage([(key, value)]))`. Until it is first set, the key is absent and
	/// the 2 KiB default applies; `system.killStorage([key])` restores the default.
	pub storage MaxCccpMessageBytes: u32 = 2 * 1024;
}

/// `pallet-cccp-relay-queue`'s `SocketQueue` hook without `pallet-btc-socket-queue`: the
/// relay queue only uses it for the message size limit. BTC-outbound socket messages
/// don't exist on this chain, so verifying one always fails.
pub struct CccpMessageLimits;
impl bp_cccp::traits::SocketVerifier<AccountId> for CccpMessageLimits {
	fn verify_socket_message(
		_msg: &bp_cccp::UnboundedBytes,
	) -> Result<(), sp_runtime::DispatchError> {
		Err(sp_runtime::DispatchError::Other("BTC socket messages are not supported"))
	}

	fn get_max_socket_message_bytes() -> u32 {
		MaxCccpMessageBytes::get()
	}
}

impl pallet_cccp_relay_queue::Config for Runtime {
	type Currency = Balances;
	type Signature = EthereumSignature;
	type Signer = EthereumSigner;
	type Relayers = RelayManager;
	type SocketQueue = CccpMessageLimits;
	type WeightInfo = pallet_cccp_relay_queue::weights::SubstrateWeight<Runtime>;
}

/// Oracle registry used by the CCCP relayers (asset / chain oracle lookups).
impl pallet_oracle_registry::Config for Runtime {
	type WeightInfo = pallet_oracle_registry::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	/// The round length reported to relayers via `round_info()`. Rounds only change with the
	/// authority set here, so this is nominal — relayers size their bootstrap look-back with it.
	pub const NominalRoundLength: u32 = 3 * HOURS;
}

/// Whether the validator has session keys registered.
pub struct HasSessionKeys;
impl Contains<AccountId> for HasSessionKeys {
	fn contains(who: &AccountId) -> bool {
		pallet_session::NextKeys::<Runtime>::contains_key(who)
	}
}

/// Root-managed validator/relayer set, replacing DPoS staking. The CCCP round only advances
/// when the set (or a relayer address) actually changes.
impl pallet_permissioned_authority::Config for Runtime {
	type RelayManager = RelayManager;
	type HasSessionKeys = HasSessionKeys;
	type RelayerSets = RelayManager;
	type MaxAuthorities = MaxAuthorities;
	type NominalRoundLength = NominalRoundLength;
	type SessionLength = SessionPeriod;
	type WeightInfo = pallet_permissioned_authority::weights::SubstrateWeight<Runtime>;
}

/// A stateless module with helpers for dispatch management.
impl pallet_utility::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = pallet_utility::weights::SubstrateWeight<Runtime>;
}

/// Account blocking (enforced by `CheckBlockedOrigin`, the precompile set and the
/// self-contained Ethereum call checks) and native token minting, both root-only here.
impl pallet_bfc_utility::Config for Runtime {
	type Currency = Balances;
	type MintableOrigin = EnsureRoot<AccountId>;
}

parameter_types! {
	/// Unused by any public EVM network (checked against chainid.network); `0xbfc0f1`.
	pub const PrivateMainnetHubChainId: u64 = 12_566_769;
	pub BlockGasLimit: U256 = U256::from(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT.ref_time() / WEIGHT_PER_GAS);
	pub WeightPerGas: Weight = Weight::from_parts(WEIGHT_PER_GAS, 0);
	pub PrecompilesValue: Precompiles = BifrostPrecompiles::<_>::new();
	/// The amount of gas per pov. A ratio of 4 if we convert ref_time to gas and we compare
	/// it with the pov_size for a block. E.g.
	/// ceil(
	///     (max_extrinsic.ref_time() / max_extrinsic.proof_size()) / WEIGHT_PER_GAS
	/// )
	pub const GasLimitPovSizeRatio: u64 = 0;
	/// BlockGasLimit / MAX_STORAGE_GROWTH = 60_000_000 / (400 * 1024) = 146
	pub const GasLimitStorageGrowthRatio: u64 = 146;
}

pub struct FindAuthorAccountId<F>(sp_std::marker::PhantomData<F>);
impl<F: FindAuthor<u32>> FindAuthor<H160> for FindAuthorAccountId<F> {
	fn find_author<'a, I>(digests: I) -> Option<H160>
	where
		I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
	{
		if let Some(author_index) = F::find_author(digests) {
			let authority_id =
				pallet_aura::Authorities::<Runtime>::get()[author_index as usize].clone();
			let queued_keys = <pallet_session::Pallet<Runtime>>::queued_keys();
			for key in queued_keys {
				if key.1.aura == authority_id {
					return Some(key.0.into());
				}
			}
		}
		None
	}
}

pub struct TransactionConverter;
impl fp_rpc::ConvertTransaction<UncheckedExtrinsic> for TransactionConverter {
	fn convert_transaction(&self, transaction: pallet_ethereum::Transaction) -> UncheckedExtrinsic {
		UncheckedExtrinsic::new_bare(
			pallet_ethereum::Call::<Runtime>::transact { transaction }.into(),
		)
	}
}
impl fp_rpc::ConvertTransaction<opaque::UncheckedExtrinsic> for TransactionConverter {
	fn convert_transaction(
		&self,
		transaction: pallet_ethereum::Transaction,
	) -> opaque::UncheckedExtrinsic {
		let extrinsic = UncheckedExtrinsic::new_bare(
			pallet_ethereum::Call::<Runtime>::transact { transaction }.into(),
		);
		let encoded = extrinsic.encode();
		opaque::UncheckedExtrinsic::decode(&mut &encoded[..])
			.expect("Encoded extrinsic is always valid")
	}
}

pub struct FixedGasPrice;
impl FeeCalculator for FixedGasPrice {
	fn min_gas_price() -> (U256, Weight) {
		(pallet_base_fee::Pallet::<Runtime>::min_gas_price().0, Weight::zero())
	}
}

/// The EVM module allows unmodified EVM code to be executed in a Substrate-based blockchain.
impl pallet_evm::Config for Runtime {
	type AccountProvider = pallet_evm::FrameSystemAccountProvider<Self>;
	type Currency = Balances;
	type BlockGasLimit = BlockGasLimit;
	type ChainId = PrivateMainnetHubChainId;
	type BlockHashMapping = EthereumBlockHashMapping<Self>;
	type Runner = pallet_evm::runner::stack::Runner<Self>;
	type CallOrigin = EnsureAddressRoot<AccountId>;
	type WithdrawOrigin = EnsureAddressNever<AccountId>;
	type AddressMapping = IdentityAddressMapping;
	type FeeCalculator = FixedGasPrice;
	type GasWeightMapping = pallet_evm::FixedGasWeightMapping<Self>;
	type WeightPerGas = WeightPerGas;
	// Native-token gas only (no `evm-tx-payment` ERC20 path); fees are burned, same as
	// `pallet_transaction_payment` above.
	type OnChargeTransaction = EVMCurrencyAdapter<Balances, ()>;
	type FindAuthor = FindAuthorAccountId<Aura>;
	type PrecompilesType = BifrostPrecompiles<Self>;
	type PrecompilesValue = PrecompilesValue;
	type OnCreate = ();
	type GasLimitPovSizeRatio = GasLimitPovSizeRatio;
	type GasLimitStorageGrowthRatio = GasLimitStorageGrowthRatio;
	type Timestamp = Timestamp;
	type CreateInnerOriginFilter = ();
	type CreateOriginFilter = ();
	type WeightInfo = pallet_evm::weights::SubstrateWeight<Runtime>;
	type FeelessCallFilter = bifrost_common_runtime::TrancheRecorderFeelessCalls<
		bifrost_common_runtime::TxRegistryRecorder<Runtime>,
	>;
}

parameter_types! {
	pub const PostBlockAndTxnHashes: PostLogContent = PostLogContent::BlockAndTxnHashes;
}

/// The Ethereum module is responsible for storing block data and provides RPC compatibility.
impl pallet_ethereum::Config for Runtime {
	type StateRoot = pallet_ethereum::IntermediateStateRoot<Self::Version>;
	type PostLogContent = PostBlockAndTxnHashes;
	type ExtraDataLength = ConstU32<30>;
}

parameter_types! {
	pub DefaultBaseFeePerGas: U256 = (1_000 * SUPPLY_FACTOR * GWEI).into();
	pub DefaultElasticity: Permill = Permill::zero();
}

pub struct BaseFeeThreshold;
impl pallet_base_fee::BaseFeeThreshold for BaseFeeThreshold {
	fn lower() -> Permill {
		Permill::zero()
	}
	fn ideal() -> Permill {
		Permill::from_parts(500_000)
	}
	fn upper() -> Permill {
		Permill::from_parts(1_000_000)
	}
}

/// The Base fee module adds support for EIP-1559 transactions and handles base fee calculations.
impl pallet_base_fee::Config for Runtime {
	type Threshold = BaseFeeThreshold;
	type DefaultBaseFeePerGas = DefaultBaseFeePerGas;
	type DefaultElasticity = DefaultElasticity;
}

impl pallet_tranche_system::Config for Runtime {
	type ProductAdminOrigin = pallet_tranche_system::EnsureProductAdmin<Runtime>;
	type WeightInfo = pallet_tranche_system::weights::SubstrateWeight<Runtime>;
}

impl pallet_tranche_permissions::Config for Runtime {
	type Vaults = TrancheSystem;
	type Products = TrancheSystem;
	type WeightInfo = pallet_tranche_permissions::weights::SubstrateWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = TranchePermissionsBenchmarkHelper;
}

/// Seeds the pallet-tranche-system state that `pallet-tranche-permissions`
/// inspects (through `type Vaults`/`type Products`) so the `TrancheInvestor`
/// grant/revoke benchmarks reach their storage write. Only compiled for benchmarks.
#[cfg(feature = "runtime-benchmarks")]
pub struct TranchePermissionsBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_tranche_permissions::BenchmarkHelper for TranchePermissionsBenchmarkHelper {
	fn setup_multichain_vault(
		product_id: pallet_tranche_system::ProductId,
		vault: pallet_tranche_system::VaultId,
	) {
		pallet_tranche_system::Vaults::<Runtime>::insert(
			vault,
			pallet_tranche_system::VaultRegistration { product_id, removed: false },
		);
	}
}

// v1 investments/tx-registry are kept (not just v2) because they own the shared
// Valuation-contract and tx-recorder identities that v2 and custom-flows read
// (`EnsureValuation`/`EnsureTxRecorder`), and so this chain exposes the exact same
// pallet/precompile surface as the public Bifrost Hub.
impl pallet_tranche_investments::Config for Runtime {
	type ValuationOrigin = pallet_tranche_investments::EnsureValuation<Runtime>;
	type Vaults = TrancheSystem;
	type Adapters = TrancheSystem;
	type WeightInfo = pallet_tranche_investments::weights::SubstrateWeight<Runtime>;
}

impl pallet_tranche_investments_v2::Config for Runtime {
	type ValuationOrigin = pallet_tranche_investments::EnsureValuation<Runtime>;
	type Vaults = TrancheSystem;
	type Adapters = TrancheSystem;
	type WeightInfo = pallet_tranche_investments_v2::weights::SubstrateWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = TrancheV2BenchmarkHelper;
}

/// Seeds pallet-tranche-system's reverse indexes so the `record_*` benchmarks
/// for both `pallet-tranche-investments-v2` and `pallet-tranche-tx-registry-v2`
/// reach their bodies. Benchmarks only.
#[cfg(feature = "runtime-benchmarks")]
pub struct TrancheV2BenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_tranche_investments_v2::BenchmarkHelper for TrancheV2BenchmarkHelper {
	fn register_vault(
		product_id: pallet_tranche_system::ProductId,
		vault: pallet_tranche_system::VaultId,
	) {
		pallet_tranche_system::Vaults::<Runtime>::insert(
			vault,
			pallet_tranche_system::VaultRegistration { product_id, removed: false },
		);
	}
	fn register_multichain_adapter(
		product_id: pallet_tranche_system::ProductId,
		key: pallet_tranche_system::AdapterKey,
	) {
		pallet_tranche_system::MultichainAdapterIndex::<Runtime>::insert(key, product_id);
	}
	fn register_adapter(
		product_id: pallet_tranche_system::ProductId,
		key: pallet_tranche_system::AdapterKey,
	) {
		pallet_tranche_system::AdapterIndex::<Runtime>::insert(key, product_id);
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl pallet_tranche_tx_registry_v2::BenchmarkHelper for TrancheV2BenchmarkHelper {
	fn register_vault(
		product_id: pallet_tranche_system::ProductId,
		vault: pallet_tranche_system::VaultId,
	) {
		pallet_tranche_system::Vaults::<Runtime>::insert(
			vault,
			pallet_tranche_system::VaultRegistration { product_id, removed: false },
		);
	}
	fn seed_recorder() {
		pallet_tranche_tx_registry::TxRecorder::<Runtime>::put(AccountId::from([0x11u8; 20]));
	}
}

impl pallet_tranche_tx_registry::Config for Runtime {
	type RecorderOrigin = pallet_tranche_tx_registry::EnsureTxRecorder<Runtime>;
	type Vaults = TrancheSystem;
	type Adapters = TrancheSystem;
	type Products = TrancheSystem;
	type WeightInfo = pallet_tranche_tx_registry::weights::SubstrateWeight<Runtime>;
}

impl pallet_tranche_tx_registry_v2::Config for Runtime {
	type RecorderOrigin = pallet_tranche_tx_registry::EnsureTxRecorder<Runtime>;
	type Vaults = TrancheSystem;
	type Adapters = TrancheSystem;
	type Products = TrancheSystem;
	type WeightInfo = pallet_tranche_tx_registry_v2::weights::SubstrateWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = TrancheV2BenchmarkHelper;
}

impl pallet_tranche_custom_flows::Config for Runtime {
	// Shares one recorder identity with pallet-tranche-tx-registry.
	type RecorderOrigin = pallet_tranche_tx_registry::EnsureTxRecorder<Runtime>;
	type GovernanceOrigin = EnsureRoot<AccountId>;
	type WeightInfo = pallet_tranche_custom_flows::weights::SubstrateWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = TrancheCustomFlowsBenchmarkHelper;
}

/// Sets `pallet-tranche-tx-registry`'s `TxRecorder` (the shared recorder
/// identity) so `record_flow_tx`'s `EnsureTxRecorder` origin resolves. Benchmarks only.
#[cfg(feature = "runtime-benchmarks")]
pub struct TrancheCustomFlowsBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_tranche_custom_flows::BenchmarkHelper for TrancheCustomFlowsBenchmarkHelper {
	fn seed_recorder() {
		pallet_tranche_tx_registry::TxRecorder::<Runtime>::put(AccountId::from([0x11u8; 20]));
	}
}

// Create the runtime by composing the FRAME pallets that were previously configured.
// Pallet indices match the public Bifrost runtimes for every pallet they share, so call
// encodings (and tooling built on them) are identical across the two chains.
#[frame_support::runtime]
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

	#[runtime::pallet_index(2)]
	pub type Timestamp = pallet_timestamp;

	#[runtime::pallet_index(3)]
	pub type Aura = pallet_aura;

	#[runtime::pallet_index(4)]
	pub type Authorship = pallet_authorship;

	#[runtime::pallet_index(5)]
	pub type Session = pallet_session;

	#[runtime::pallet_index(6)]
	pub type Historical = pallet_session::historical;

	#[runtime::pallet_index(7)]
	pub type Offences = pallet_offences;

	#[runtime::pallet_index(8)]
	pub type ImOnline = pallet_im_online;

	#[runtime::pallet_index(9)]
	pub type Grandpa = pallet_grandpa;

	#[runtime::pallet_index(10)]
	pub type Balances = pallet_balances;

	#[runtime::pallet_index(11)]
	pub type TransactionPayment = pallet_transaction_payment;

	#[runtime::pallet_index(20)]
	pub type RelayManager = pallet_relay_manager;

	#[runtime::pallet_index(22)]
	pub type BfcUtility = pallet_bfc_utility;

	#[runtime::pallet_index(24)]
	pub type PermissionedAuthority = pallet_permissioned_authority;

	#[runtime::pallet_index(30)]
	pub type Utility = pallet_utility;

	#[runtime::pallet_index(32)]
	pub type SafeMode = pallet_safe_mode;

	#[runtime::pallet_index(33)]
	pub type TxPause = pallet_tx_pause;

	#[runtime::pallet_index(40)]
	pub type EVM = pallet_evm;

	#[runtime::pallet_index(41)]
	pub type Ethereum = pallet_ethereum;

	#[runtime::pallet_index(42)]
	pub type BaseFee = pallet_base_fee;

	#[runtime::pallet_index(64)]
	pub type CCCPRelayQueue = pallet_cccp_relay_queue;

	#[runtime::pallet_index(65)]
	pub type OracleRegistry = pallet_oracle_registry;

	#[runtime::pallet_index(80)]
	pub type TrancheSystem = pallet_tranche_system;

	#[runtime::pallet_index(81)]
	pub type TranchePermissions = pallet_tranche_permissions;

	#[runtime::pallet_index(82)]
	pub type TrancheInvestments = pallet_tranche_investments;

	#[runtime::pallet_index(83)]
	pub type TrancheTxRegistry = pallet_tranche_tx_registry;

	#[runtime::pallet_index(84)]
	pub type TrancheCustomFlows = pallet_tranche_custom_flows;

	#[runtime::pallet_index(85)]
	pub type TrancheTxRegistryV2 = pallet_tranche_tx_registry_v2;

	#[runtime::pallet_index(86)]
	pub type TrancheInvestmentsV2 = pallet_tranche_investments_v2;

	#[runtime::pallet_index(99)]
	pub type Sudo = pallet_sudo;
}

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	frame_benchmarking::define_benchmarks!(
		[frame_system, SystemBench::<Runtime>]
		[pallet_tranche_system, TrancheSystem]
		[pallet_tranche_permissions, TranchePermissions]
		[pallet_tranche_investments_v2, TrancheInvestmentsV2]
		[pallet_tranche_tx_registry_v2, TrancheTxRegistryV2]
		[pallet_tranche_custom_flows, TrancheCustomFlows]
	);
}

bifrost_common_runtime::impl_common_runtime_apis!();
bifrost_common_runtime::impl_self_contained_call!();
