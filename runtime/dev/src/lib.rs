// Build both the Native Rust binary and the WASM binary.
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

pub use bifrost_dev_constants::{
	currency::{GWEI, UNITS as BFC, *},
	fee::*,
	time::*,
};

use bp_btc_relay::Network;
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
		BlakeTwo256, Block as BlockT, ConvertInto, DispatchInfoOf, Dispatchable, IdentityLookup,
		NumberFor, OpaqueKeys, PostDispatchInfoOf, UniqueSaturatedInto,
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

pub use pallet_balances::{Call as BalancesCall, NegativeImbalance};
pub use pallet_bfc_staking::{InflationInfo, Range};
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
use pallet_identity::legacy::IdentityInfo;
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
	traits::{
		fungible::HoldConsideration,
		tokens::{
			fungible::Credit, imbalance::ResolveTo, PayFromAccount, UnityAssetBalanceConversion,
		},
		ConstU128, ConstU32, ConstU8, Contains, Currency, EitherOfDiverse, EqualPrivilegeOnly,
		FindAuthor, Imbalance, InsideBoth, KeyOwnerProofSystem, LinearStoragePrice, LockIdentifier,
		NeverEnsureOrigin, OnFinalize, OnUnbalanced, Randomness, StorageInfo,
	},
	weights::{
		constants::{
			BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND,
		},
		ConstantMultiplier, IdentityFee, Weight, WeightToFeeCoefficient, WeightToFeeCoefficients,
		WeightToFeePolynomial,
	},
	ConsensusEngineId, PalletId, StorageValue,
};
use frame_system::{EnsureRoot, EnsureRootWithSuccess, EnsureSigned};

mod precompiles;
pub use precompiles::BifrostPrecompiles;

pub type Precompiles = BifrostPrecompiles<Runtime>;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// The `TransactionExtension` to the basic transaction logic.
pub type TxExtension = (
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

/// All migrations executed on runtime upgrade as a nested tuple of types implementing `OnRuntimeUpgrade`.
type Migrations = (
	pallet_session::migrations::v1::MigrateV0ToV1<
		Runtime,
		pallet_session::migrations::v1::InitOffenceSeverity<Runtime>,
	>,
);

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	Migrations,
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
	spec_name: alloc::borrow::Cow::Borrowed("thebifrost-dev"),
	// The name of the implementation of the spec.
	impl_name: alloc::borrow::Cow::Borrowed("bifrost-dev"),
	// The version of the authorship interface.
	authoring_version: 1,
	// The version of the runtime spec.
	spec_version: 396,
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
/// We allow for 1 second of compute with a 3 second average block time, with maximum proof size.
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
	type BaseCallFilter = InsideBoth<SafeMode, TxPause>;
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
	/// migrations pallet
	type MultiBlockMigrator = MultiBlockMigrations;
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
			| RuntimeCall::RelayManager(pallet_relay_manager::Call::heartbeat { .. }) => true,
			_ => false,
		}
	}
}

impl pallet_tx_pause::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PauseOrigin = EnsureRoot<AccountId>;
	type UnpauseOrigin = EnsureRoot<AccountId>;
	type WhitelistedCalls = ();
	type MaxNameLen = ConstU32<256>;
	type WeightInfo = pallet_tx_pause::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const EnterDuration: BlockNumber = 2 * MINUTES;
	pub const EnterDepositAmount: Balance = 2_000 * BFC;
	pub const ExtendDuration: BlockNumber = 1 * MINUTES;
	pub const ExtendDepositAmount: Balance = 1_000 * BFC;
	pub const ReleaseDelay: u32 = 1 * MINUTES;
}

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
	pub const MaxAuthorities: u32 = 1_000;
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
	type MaxNominators = ConstU32<150>;
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

pub struct DealWithFees<R>(sp_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for DealWithFees<R>
where
	R: pallet_balances::Config + pallet_treasury::Config,
	pallet_treasury::Pallet<R>: OnUnbalanced<NegativeImbalance<R>>,
{
	// this seems to be called for substrate-based transactions
	fn on_unbalanceds(mut fees_then_tips: impl Iterator<Item = NegativeImbalance<R>>) {
		if let Some(fees) = fees_then_tips.next() {
			// for fees, 20% are burned, 80% to the treasury
			let (_, to_treasury) = fees.ration(20, 80);
			// Balances pallet automatically burns dropped Negative Imbalances by decreasing
			// total_supply accordingly
			<pallet_treasury::Pallet<R> as OnUnbalanced<_>>::on_unbalanced(to_treasury);
		}
	}

	// this is called from pallet_evm for Ethereum-based transactions
	// (technically, it calls on_unbalanced, which calls this when non-zero)
	fn on_nonzero_unbalanced(amount: NegativeImbalance<R>) {
		// Balances pallet automatically burns dropped Negative Imbalances by decreasing
		// total_supply accordingly
		let (_, to_treasury) = amount.ration(20, 80);
		<pallet_treasury::Pallet<R> as OnUnbalanced<_>>::on_unbalanced(to_treasury);
	}
}

parameter_types! {
	pub const TransactionByteFee: Balance = TRANSACTION_BYTE_FEE;
}

/// Provides the basic logic needed to pay the absolute minimum amount needed for a transaction to
/// be included.
#[allow(deprecated)]
impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = CurrencyAdapter<Balances, DealWithFees<Runtime>>;
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
	pub MaximumSchedulerWeight: Weight = NORMAL_DISPATCH_RATIO * BlockWeights::get().max_block;
	pub const MaxScheduledPerBlock: u32 = 50;
}

/// The Scheduler module exposes capabilities for scheduling dispatches to occur at a
/// specified block number or at a specified period.
impl pallet_scheduler::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaximumSchedulerWeight;
	type ScheduleOrigin = EnsureRoot<AccountId>;
	type MaxScheduledPerBlock = MaxScheduledPerBlock;
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type WeightInfo = pallet_scheduler::weights::SubstrateWeight<Runtime>;
	type BlockNumberProvider = System;
	type Preimages = Preimage;
}

parameter_types! {
	pub const SessionPeriod: u32 = 1 * MINUTES;
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
	type SessionManager = BfcStaking;
	type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = opaque::SessionKeys;
	type DisablingStrategy = ();
	type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
}

impl pallet_session::historical::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type FullIdentification = pallet_bfc_staking::ValidatorSnapshot<AccountId, Balance>;
	type FullIdentificationOf = pallet_bfc_staking::ValidatorSnapshotOf<Self>;
}

/// The Authorship module tracks the current author of the block.
impl pallet_authorship::Config for Runtime {
	type EventHandler = BfcStaking;
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
}

/// A stateless module with helpers for dispatch management.
impl pallet_utility::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = pallet_utility::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	/// The maximum amount of time (in blocks) for council members to vote on motions.
	/// Motions may end in fewer blocks if enough votes are cast to determine the result.
	pub const CouncilMotionDuration: BlockNumber = 1 * HOURS;
	/// The maximum number of Proposals that can be open in the council at once.
	pub const CouncilMaxProposals: u32 = 100;
	/// The maximum number of council members.
	pub const CouncilMaxMembers: u32 = 100;

	/// The maximum amount of time (in blocks) for technical committee members to vote on motions.
	/// Motions may end in fewer blocks if enough votes are cast to determine the result.
	pub const TechCommitteeMotionDuration: BlockNumber = 1 * HOURS;
	/// The maximum number of Proposals that can be open in the technical committee at once.
	pub const TechCommitteeMaxProposals: u32 = 100;
	/// The maximum number of technical committee members.
	pub const TechCommitteeMaxMembers: u32 = 100;

	/// The maximum amount of time (in blocks) for relay executive members to vote on motions.
	/// Motions may end in fewer blocks if enough votes are cast to determine the result.
	pub const RelayExecutivesMotionDuration: BlockNumber = 1 * HOURS;
	/// The maximum number of Proposals that can be open in the relay executives at once.
	pub const RelayExecutivesMaxProposals: u32 = 10;
	/// The maximum number of relay executive members.
	pub const RelayExecutivesMaxMembers: u32 = 10;

	pub MaxProposalWeight: Weight = BlockWeights::get().max_block;
}

/// A type that represents a council member for governance
type CouncilInstance = pallet_collective::Instance1;

/// A module that grants council members to participate for governance
impl pallet_collective::Config<CouncilInstance> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeEvent = RuntimeEvent;
	type Proposal = RuntimeCall;
	type MotionDuration = CouncilMotionDuration;
	type MaxProposals = CouncilMaxProposals;
	type MaxMembers = CouncilMaxMembers;
	type DefaultVote = pallet_collective::MoreThanMajorityThenPrimeDefaultVote;
	type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
	type SetMembersOrigin = EnsureRoot<Self::AccountId>;
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<Self::AccountId>;
	type KillOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
}

/// A type that represents a technical committee member for governance
type TechCommitteeInstance = pallet_collective::Instance2;

/// A module that grants technical committee members to participate for governance
impl pallet_collective::Config<TechCommitteeInstance> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeEvent = RuntimeEvent;
	type Proposal = RuntimeCall;
	type MotionDuration = TechCommitteeMotionDuration;
	type MaxProposals = TechCommitteeMaxProposals;
	type MaxMembers = TechCommitteeMaxMembers;
	type DefaultVote = pallet_collective::MoreThanMajorityThenPrimeDefaultVote;
	type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
	type SetMembersOrigin = EnsureRoot<Self::AccountId>;
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<Self::AccountId>;
	type KillOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
}

/// A type that represents a relay executive member for governance
type RelayExecutiveInstance = pallet_collective::Instance3;

/// A module that grants relay executive members to participate for governance
impl pallet_collective::Config<RelayExecutiveInstance> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeEvent = RuntimeEvent;
	type Proposal = RuntimeCall;
	type MotionDuration = RelayExecutivesMotionDuration;
	type MaxProposals = RelayExecutivesMaxProposals;
	type MaxMembers = RelayExecutivesMaxMembers;
	type DefaultVote = pallet_collective::MoreThanMajorityThenPrimeDefaultVote;
	type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
	type SetMembersOrigin = EnsureRoot<Self::AccountId>;
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<Self::AccountId>;
	type KillOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
}

type MoreThanHalfCouncil = EitherOfDiverse<
	EnsureRoot<AccountId>,
	pallet_collective::EnsureProportionMoreThan<AccountId, CouncilInstance, 1, 2>,
>;

/// A module that manages council member authorities
impl pallet_membership::Config<pallet_membership::Instance1> for Runtime {
	type AddOrigin = MoreThanHalfCouncil;
	type RuntimeEvent = RuntimeEvent;
	type MaxMembers = CouncilMaxMembers;
	type MembershipChanged = Council;
	type MembershipInitialized = Council;
	type PrimeOrigin = MoreThanHalfCouncil;
	type RemoveOrigin = MoreThanHalfCouncil;
	type ResetOrigin = MoreThanHalfCouncil;
	type SwapOrigin = MoreThanHalfCouncil;
	type WeightInfo = ();
}

/// A module that manages technical committee member authorities
impl pallet_membership::Config<pallet_membership::Instance2> for Runtime {
	type AddOrigin = MoreThanHalfCouncil;
	type RuntimeEvent = RuntimeEvent;
	type MaxMembers = TechCommitteeMaxMembers;
	type MembershipChanged = TechnicalCommittee;
	type MembershipInitialized = TechnicalCommittee;
	type PrimeOrigin = MoreThanHalfCouncil;
	type RemoveOrigin = MoreThanHalfCouncil;
	type ResetOrigin = MoreThanHalfCouncil;
	type SwapOrigin = MoreThanHalfCouncil;
	type WeightInfo = ();
}

type MoreThanTwoThirdsRelayExecutives = EitherOfDiverse<
	EnsureRoot<AccountId>,
	pallet_collective::EnsureProportionAtLeast<AccountId, RelayExecutiveInstance, 2, 3>,
>;

/// A module that manages relay executive member authorities
impl pallet_membership::Config<pallet_membership::Instance3> for Runtime {
	type AddOrigin = MoreThanTwoThirdsRelayExecutives;
	type RuntimeEvent = RuntimeEvent;
	type MaxMembers = RelayExecutivesMaxMembers;
	type MembershipChanged = RelayExecutive;
	type MembershipInitialized = RelayExecutive;
	type PrimeOrigin = MoreThanTwoThirdsRelayExecutives;
	type RemoveOrigin = MoreThanTwoThirdsRelayExecutives;
	type ResetOrigin = MoreThanTwoThirdsRelayExecutives;
	type SwapOrigin = MoreThanTwoThirdsRelayExecutives;
	type WeightInfo = ();
}

parameter_types! {
	pub const LaunchPeriod: BlockNumber = 1 * MINUTES;
	pub const VotingPeriod: BlockNumber = 1 * MINUTES;
	pub const VoteLockingPeriod: BlockNumber = 1 * MINUTES;
	pub const FastTrackVotingPeriod: BlockNumber = 1 * MINUTES;
	pub const EnactmentPeriod: BlockNumber = 1 * MINUTES;
	pub const CooloffPeriod: BlockNumber = 3 * MINUTES;
	pub const MinimumDeposit: Balance = 4 * SUPPLY_FACTOR * BFC;
	pub const MaxVotes: u32 = 100;
	pub const MaxProposals: u32 = 100;
	pub const MaxDeposits: u32 = 1_000;
	pub const MaxBlacklisted: u32 = 1_000;
	pub const InstantAllowed: bool = true;
}

/// The core module that supports governance functionality to this network
impl pallet_democracy::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type EnactmentPeriod = EnactmentPeriod;
	type LaunchPeriod = LaunchPeriod;
	type VotingPeriod = VotingPeriod;
	type VoteLockingPeriod = VoteLockingPeriod;
	type FastTrackVotingPeriod = FastTrackVotingPeriod;
	type MinimumDeposit = MinimumDeposit;
	/// To decide what their next motion is.
	type ExternalOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilInstance, 1, 2>;
	/// To have the next scheduled referendum be a straight majority-carries vote.
	type ExternalMajorityOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilInstance, 3, 5>;
	/// To have the next scheduled referendum be a straight default-carries (NTB) vote.
	type ExternalDefaultOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilInstance, 3, 5>;
	/// To allow a shorter voting/enactment period for external proposals.
	type FastTrackOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, TechCommitteeInstance, 1, 2>;
	/// To instant fast track.
	type InstantOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, TechCommitteeInstance, 3, 5>;
	// To cancel a proposal which has been passed.
	type CancellationOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilInstance, 3, 5>,
	>;
	// To cancel a proposal before it has been passed.
	type CancelProposalOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		pallet_collective::EnsureProportionAtLeast<AccountId, TechCommitteeInstance, 3, 5>,
	>;
	type BlacklistOrigin = EnsureRoot<AccountId>;
	type SubmitOrigin = EnsureSigned<AccountId>;
	// Any single technical committee member may veto a coming council proposal, however they can
	// only do it once and it lasts only for the cooloff period.
	type VetoOrigin = pallet_collective::EnsureMember<AccountId, TechCommitteeInstance>;
	type CooloffPeriod = CooloffPeriod;
	type Slash = Treasury;
	type InstantAllowed = InstantAllowed;
	type Scheduler = Scheduler;
	type MaxVotes = MaxVotes;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = pallet_democracy::weights::SubstrateWeight<Runtime>;
	type MaxProposals = MaxProposals;
	type Preimages = Preimage;
	type MaxDeposits = MaxDeposits;
	type MaxBlacklisted = MaxBlacklisted;
}

parameter_types! {
	pub const PreimageBaseDeposit: Balance = 5 * SUPPLY_FACTOR * BFC;
	pub const PreimageByteDeposit: Balance = STORAGE_BYTE_FEE;
	pub const PreimageHoldReason: RuntimeHoldReason = RuntimeHoldReason::Preimage(pallet_preimage::HoldReason::Preimage);
}

impl pallet_preimage::Config for Runtime {
	type WeightInfo = pallet_preimage::weights::SubstrateWeight<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<AccountId>;
	type Consideration = HoldConsideration<
		AccountId,
		Balances,
		PreimageHoldReason,
		LinearStoragePrice<PreimageBaseDeposit, PreimageByteDeposit, Balance>,
	>;
}

parameter_types! {
	pub const ProposalBond: Permill = Permill::from_percent(5);
	pub const ProposalBondMinimum: Balance = 1 * BFC;
	pub const SpendPeriod: BlockNumber = 1 * MINUTES;
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const MaxApprovals: u32 = 100;
	pub TreasuryAccount: AccountId = Treasury::account_id();
}

/// A module that manages funds stored in a certain vault
impl pallet_treasury::Config for Runtime {
	type PalletId = TreasuryPalletId;
	type Currency = Balances;
	type RejectOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		pallet_collective::EnsureProportionMoreThan<AccountId, CouncilInstance, 1, 2>,
	>;
	type RuntimeEvent = RuntimeEvent;
	type SpendPeriod = SpendPeriod;
	type Burn = ();
	type BurnDestination = ();
	type WeightInfo = pallet_treasury::weights::SubstrateWeight<Runtime>;
	type SpendFunds = ();
	type MaxApprovals = MaxApprovals;
	type SpendOrigin = NeverEnsureOrigin<Balance>;
	type AssetKind = ();
	type Beneficiary = AccountId;
	type BeneficiaryLookup = IdentityLookup<AccountId>;
	type Paymaster = PayFromAccount<Balances, TreasuryAccount>;
	type BalanceConverter = UnityAssetBalanceConversion;
	type PayoutPeriod = ConstU32<0>;
	type BlockNumberProvider = System;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BenchmarkHelper;
}

parameter_types! {
	pub const BasicDeposit: Balance = 100 * BFC;
	pub const ByteDeposit: Balance = 100 * BFC;
	pub const SubAccountDeposit: Balance = 100 * BFC;
	pub const UsernameDeposit: Balance = 100 * BFC;
	pub const MaxSubAccounts: u32 = 100;
	pub const MaxAdditionalFields: u32 = 100;
	pub const MaxRegistrars: u32 = 20;
	pub const PendingUsernameExpiration: u32 = 1 * MINUTES;
	pub const UsernameGracePeriod: u32 = 1 * MINUTES;
	pub const MaxSuffixLength: u32 = 7;
	pub const MaxUsernameLength: u32 = 32;
}

/// The module that manages account identities and registrar judgements.
impl pallet_identity::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BasicDeposit = BasicDeposit;
	type ByteDeposit = ByteDeposit;
	type SubAccountDeposit = SubAccountDeposit;
	type UsernameDeposit = UsernameDeposit;
	type MaxSubAccounts = MaxSubAccounts;
	type IdentityInformation = IdentityInfo<MaxAdditionalFields>;
	type MaxRegistrars = MaxRegistrars;
	type Slashed = Treasury;
	type ForceOrigin = EnsureRoot<AccountId>;
	type RegistrarOrigin = EnsureRoot<AccountId>;
	type OffchainSignature = Signature;
	type SigningPublicKey = <Signature as traits::Verify>::Signer;
	type UsernameAuthorityOrigin = EnsureRoot<Self::AccountId>;
	type PendingUsernameExpiration = PendingUsernameExpiration;
	type UsernameGracePeriod = UsernameGracePeriod;
	type MaxSuffixLength = MaxSuffixLength;
	type MaxUsernameLength = MaxUsernameLength;
	type WeightInfo = pallet_identity::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const ImOnlineUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
	pub const MaxKeys: u32 = 10_000;
	pub const MaxPeerInHeartbeats: u32 = 10_000;
	pub const DefaultSlashFraction: Perbill = Perbill::from_parts(5_000_000); // 0.5%
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
	type NextSessionRotation = BfcStaking;
	type ValidatorSet = Historical;
	type ReportUnresponsiveness = Offences;
	type UnsignedPriority = ImOnlineUnsignedPriority;
	type WeightInfo = pallet_im_online::weights::SubstrateWeight<Runtime>;
	type MaxKeys = MaxKeys;
	type MaxPeerInHeartbeats = MaxPeerInHeartbeats;
	type DefaultSlashFraction = DefaultSlashFraction;
}

/// The module that manages validator offences.
impl pallet_offences::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type IdentificationTuple = pallet_session::historical::IdentificationTuple<Self>;
	type OnOffenceHandler = BfcStaking;
}

parameter_types! {
	pub const DefaultOffenceExpirationInSessions: u32 = 5u32;
	pub const DefaultFullMaximumOffenceCount: u32 = 5u32;
	pub const DefaultBasicMaximumOffenceCount: u32 = 3u32;
	pub const IsOffenceActive: bool = true;
	pub const IsSlashActive: bool = true;
}

/// A module that wraps `pallet_offences` to act as a central offence handler
impl pallet_bfc_offences::Config for Runtime {
	type Currency = Balances;
	type Slash = Treasury;
	type DefaultOffenceExpirationInSessions = DefaultOffenceExpirationInSessions;
	type DefaultFullMaximumOffenceCount = DefaultFullMaximumOffenceCount;
	type DefaultBasicMaximumOffenceCount = DefaultBasicMaximumOffenceCount;
	type IsOffenceActive = IsOffenceActive;
	type IsSlashActive = IsSlashActive;
	type WeightInfo = pallet_bfc_offences::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const StorageCacheLifetimeInRounds: u32 = 64u32;
	pub const IsHeartbeatOffenceActive: bool = false;
	pub const DefaultHeartbeatSlashFraction: Perbill = Perbill::from_percent(1);
}

/// A module that manages registered relayers for cross chain interoperability
impl pallet_relay_manager::Config for Runtime {
	type SocketQueue = BtcSocketQueue;
	type RegistrationPool = BtcRegistrationPool;
	type ValidatorSet = Historical;
	type ReportUnresponsiveness = Offences;
	type StorageCacheLifetimeInRounds = StorageCacheLifetimeInRounds;
	type IsHeartbeatOffenceActive = IsHeartbeatOffenceActive;
	type DefaultHeartbeatSlashFraction = DefaultHeartbeatSlashFraction;
	type WeightInfo = pallet_relay_manager::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	/// Minimum round length that can be set by the system.
	pub const MinBlocksPerRound: u32 = 1;
	/// Maximum round length that can be set by the system.
	pub const MaxBlocksPerRound: u32 = 28 * DAYS;
	/// Blocks per round.
	pub const DefaultBlocksPerRound: u32 = 2 * MINUTES;
	/// Rounds before the validator leaving the candidates request can be executed.
	pub const LeaveCandidatesDelay: u32 = 1;
	/// Rounds before the candidate bond increase/decrease can be executed.
	pub const CandidateBondLessDelay: u32 = 1;
	/// Rounds before the nominator exit can be executed.
	pub const LeaveNominatorsDelay: u32 = 1;
	/// Rounds before the nominator revocation can be executed.
	pub const RevokeNominationDelay: u32 = 1;
	/// Rounds before the nominator bond increase/decrease can be executed.
	pub const NominationBondLessDelay: u32 = 1;
	/// Rounds before the reward is paid.
	pub const RewardPaymentDelay: u32 = 1;
	/// Default maximum full validators selected per round, default at genesis.
	pub const DefaultMaxSelectedFullCandidates: u32 = 10;
	/// Default maximum basicvalidators selected per round, default at genesis.
	pub const DefaultMaxSelectedBasicCandidates: u32 = 10;
	/// Maximum top nominations per candidate.
	pub const MaxTopNominationsPerCandidate: u32 = 2;
	/// Maximum bottom nominations per candidate.
	pub const MaxBottomNominationsPerCandidate: u32 = 1;
	/// Maximum nominations per nominator.
	pub const MaxNominationsPerNominator: u32 = 3;
	/// Default commission rate for full validators.
	pub const DefaultFullValidatorCommission: Perbill = Perbill::from_percent(50);
	/// Default commission rate for basic validators.
	pub const DefaultBasicValidatorCommission: Perbill = Perbill::from_percent(10);
	/// Maximum commission rate available for full validators.
	pub const MaxFullValidatorCommission: Perbill = Perbill::from_percent(100);
	/// Maximum commission rate available for basic validators.
	pub const MaxBasicValidatorCommission: Perbill = Perbill::from_percent(20);
	/// Minimum stake required to become a full validator.
	pub const MinFullValidatorStk: u128 = 1_000 * SUPPLY_FACTOR * BFC;
	/// Minimum stake required to become a basic validator.
	pub const MinBasicValidatorStk: u128 = 500 * SUPPLY_FACTOR * BFC;
	/// Minimum stake required to be reserved to be a full candidate.
	pub const MinFullCandidateStk: u128 = 950 * SUPPLY_FACTOR * BFC;
	/// Minimum stake required to be reserved to be a basic candidate.
	pub const MinBasicCandidateStk: u128 = 100 * SUPPLY_FACTOR * BFC;
	/// Minimum stake required to be reserved to be a nominator.
	pub const MinNominatorStk: u128 = 1 * SUPPLY_FACTOR * BFC;
}

/// Minimal staking pallet that implements validator selection by total backed stake.
impl pallet_bfc_staking::Config for Runtime {
	type Currency = Balances;
	type MonetaryGovernanceOrigin = EnsureRoot<AccountId>;
	type RelayManager = RelayManager;
	type OffenceHandler = BfcOffences;
	type MinBlocksPerRound = MinBlocksPerRound;
	type MaxBlocksPerRound = MaxBlocksPerRound;
	type DefaultBlocksPerSession = SessionPeriod;
	type DefaultBlocksPerRound = DefaultBlocksPerRound;
	type StorageCacheLifetimeInRounds = StorageCacheLifetimeInRounds;
	type LeaveCandidatesDelay = LeaveCandidatesDelay;
	type CandidateBondLessDelay = CandidateBondLessDelay;
	type LeaveNominatorsDelay = LeaveNominatorsDelay;
	type RevokeNominationDelay = RevokeNominationDelay;
	type NominationBondLessDelay = NominationBondLessDelay;
	type RewardPaymentDelay = RewardPaymentDelay;
	type DefaultMaxSelectedFullCandidates = DefaultMaxSelectedFullCandidates;
	type DefaultMaxSelectedBasicCandidates = DefaultMaxSelectedBasicCandidates;
	type MaxTopNominationsPerCandidate = MaxTopNominationsPerCandidate;
	type MaxBottomNominationsPerCandidate = MaxBottomNominationsPerCandidate;
	type MaxNominationsPerNominator = MaxNominationsPerNominator;
	type DefaultFullValidatorCommission = DefaultFullValidatorCommission;
	type DefaultBasicValidatorCommission = DefaultBasicValidatorCommission;
	type MaxFullValidatorCommission = MaxFullValidatorCommission;
	type MaxBasicValidatorCommission = MaxBasicValidatorCommission;
	type MinFullValidatorStk = MinFullValidatorStk;
	type MinBasicValidatorStk = MinBasicValidatorStk;
	type MinFullCandidateStk = MinFullCandidateStk;
	type MinBasicCandidateStk = MinBasicCandidateStk;
	type MinNomination = MinNominatorStk;
	type MinNominatorStk = MinNominatorStk;
	type WeightInfo = pallet_bfc_staking::weights::SubstrateWeight<Runtime>;
}

/// A module that manages this networks community
impl pallet_bfc_utility::Config for Runtime {
	type Currency = Balances;
	type MintableOrigin =
		pallet_collective::EnsureProportionMoreThan<AccountId, CouncilInstance, 1, 2>;
}

parameter_types! {
	pub const BifrostChainId: u64 = 49088; // 0xbfc0
	pub BlockGasLimit: U256 = U256::from(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT.ref_time() / WEIGHT_PER_GAS);
	pub WeightPerGas: Weight = Weight::from_parts(WEIGHT_PER_GAS, 0);
	pub PrecompilesValue: Precompiles = BifrostPrecompiles::<_>::new();
	/// The amount of gas per pov. A ratio of 4 if we convert ref_time to gas and we compare
	/// it with the pov_size for a block. E.g.
	/// ceil(
	///     (max_extrinsic.ref_time() / max_extrinsic.proof_size()) / WEIGHT_PER_GAS
	/// )
	pub const GasLimitPovSizeRatio: u64 = 4;
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
	type ChainId = BifrostChainId;
	type BlockHashMapping = EthereumBlockHashMapping<Self>;
	type Runner = pallet_evm::runner::stack::Runner<Self>;
	type CallOrigin = EnsureAddressRoot<AccountId>;
	type WithdrawOrigin = EnsureAddressNever<AccountId>;
	type AddressMapping = IdentityAddressMapping;
	type FeeCalculator = FixedGasPrice;
	type GasWeightMapping = pallet_evm::FixedGasWeightMapping<Self>;
	type WeightPerGas = WeightPerGas;
	type OnChargeTransaction = EVMCurrencyAdapter<Balances, DealWithFees<Runtime>>;
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

impl pallet_btc_socket_queue::Config for Runtime {
	type Signature = EthereumSignature;
	type Signer = EthereumSigner;
	type Executives = RelayExecutiveMembership;
	type Relayers = RelayManager;
	type RegistrationPool = BtcRegistrationPool;
	type Blaze = Blaze;
	type WeightInfo = pallet_btc_socket_queue::weights::SubstrateWeight<Runtime>;
	type DefaultMaxFeeRate = DefaultMaxFeeRate;
}

parameter_types! {
	pub const BitcoinChainId: u32 = 10002;
	pub const BitcoinNetwork: Network = Network::Regtest;
	pub const DefaultMultiSigRatio: Percent = Percent::from_percent(100);
	pub const DefaultMaxFeeRate: u64 = 15;
}

impl pallet_btc_registration_pool::Config for Runtime {
	type Signature = EthereumSignature;
	type Signer = EthereumSigner;
	type Executives = RelayExecutiveMembership;
	type SocketQueue = BtcSocketQueue;
	type DefaultMultiSigRatio = DefaultMultiSigRatio;
	type BitcoinChainId = BitcoinChainId;
	type BitcoinNetwork = BitcoinNetwork;
	type WeightInfo = pallet_btc_registration_pool::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const FeeRateExpiration: u32 = 1 * MINUTES;
	pub const ToleranceThreshold: u32 = 3;
}

impl pallet_blaze::Config for Runtime {
	type Signature = EthereumSignature;
	type Signer = EthereumSigner;
	type Relayers = RelayManager;
	type SocketQueue = BtcSocketQueue;
	type RegistrationPool = BtcRegistrationPool;
	type FeeRateExpiration = FeeRateExpiration;
	type ToleranceThreshold = ToleranceThreshold;
	type WeightInfo = pallet_blaze::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub MbmServiceWeight: Weight = Perbill::from_percent(80) * BlockWeights::get().max_block;
}

impl pallet_migrations::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Migrations = pallet_identity::migration::v2::LazyMigrationV1ToV2<Runtime>;
	type CursorMaxLen = ConstU32<65_536>;
	type IdentifierMaxLen = ConstU32<256>;
	type MigrationStatusHandler = ();
	type FailedMigrationHandler = frame_support::migrations::FreezeChainOnFailedMigration;
	type MaxServiceWeight = MbmServiceWeight;
	type WeightInfo = pallet_migrations::weights::SubstrateWeight<Runtime>;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
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

	#[runtime::pallet_index(21)]
	pub type BfcStaking = pallet_bfc_staking;

	#[runtime::pallet_index(22)]
	pub type BfcUtility = pallet_bfc_utility;

	#[runtime::pallet_index(23)]
	pub type BfcOffences = pallet_bfc_offences;

	#[runtime::pallet_index(30)]
	pub type Utility = pallet_utility;

	#[runtime::pallet_index(31)]
	pub type Identity = pallet_identity;

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

	#[runtime::pallet_index(50)]
	pub type Scheduler = pallet_scheduler;

	#[runtime::pallet_index(51)]
	pub type Democracy = pallet_democracy;

	#[runtime::pallet_index(52)]
	pub type Council = pallet_collective<Instance1>;

	#[runtime::pallet_index(53)]
	pub type TechnicalCommittee = pallet_collective<Instance2>;

	#[runtime::pallet_index(54)]
	pub type CouncilMembership = pallet_membership<Instance1>;

	#[runtime::pallet_index(55)]
	pub type TechnicalMembership = pallet_membership<Instance2>;

	#[runtime::pallet_index(56)]
	pub type Treasury = pallet_treasury;

	#[runtime::pallet_index(57)]
	pub type Preimage = pallet_preimage;

	#[runtime::pallet_index(58)]
	pub type RelayExecutive = pallet_collective<Instance3>;

	#[runtime::pallet_index(59)]
	pub type RelayExecutiveMembership = pallet_membership<Instance3>;

	#[runtime::pallet_index(60)]
	pub type BtcSocketQueue = pallet_btc_socket_queue;

	#[runtime::pallet_index(61)]
	pub type BtcRegistrationPool = pallet_btc_registration_pool;

	#[runtime::pallet_index(62)]
	pub type Blaze = pallet_blaze;

	#[runtime::pallet_index(99)]
	pub type Sudo = pallet_sudo;

	#[runtime::pallet_index(100)]
	pub type MultiBlockMigrations = pallet_migrations;
}

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	frame_benchmarking::define_benchmarks!(
		[frame_system, SystemBench::<Runtime>]
		[pallet_relay_manager, RelayManager]
		[pallet_blaze, Blaze]
		[pallet_btc_registration_pool, BtcRegistrationPool]
		[pallet_btc_socket_queue, BtcSocketQueue]
	);
}

bifrost_common_runtime::impl_common_runtime_apis!();
bifrost_common_runtime::impl_self_contained_call!();
