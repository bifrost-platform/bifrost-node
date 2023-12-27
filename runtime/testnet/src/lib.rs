// Build both the Native Rust binary and the WASM binary.
#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub use bp_core::{AccountId, Address, Balance, BlockNumber, Hash, Header, Nonce, Signature};
use frame_support::traits::{
	fungible::HoldConsideration,
	tokens::{PayFromAccount, UnityAssetBalanceConversion},
	LinearStoragePrice,
};
use pallet_identity::simple::IdentityInfo;
use parity_scale_codec::{Decode, Encode};

pub use bifrost_testnet_constants::{
	currency::{GWEI, UNITS as BFC, *},
	fee::*,
	time::*,
};

use fp_rpc::TransactionStatus;
use fp_rpc_txpool::TxPoolResponse;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, ConstBool, ConstU64, OpaqueMetadata, H160, H256, U256};
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{
		BlakeTwo256, Block as BlockT, ConvertInto, DispatchInfoOf, Dispatchable, IdentityLookup,
		NumberFor, OpaqueKeys, PostDispatchInfoOf, UniqueSaturatedInto,
	},
	transaction_validity::{
		TransactionPriority, TransactionSource, TransactionValidity, TransactionValidityError,
	},
	ApplyExtrinsicResult,
};
pub use sp_runtime::{Perbill, Percent, Permill};
use sp_std::prelude::*;

#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

pub use pallet_balances::{Call as BalancesCall, NegativeImbalance};
pub use pallet_bfc_staking::{InflationInfo, Range};
use pallet_ethereum::{
	Call::transact, EthereumBlockHashMapping, PostLogContent, Transaction as EthereumTransaction,
};
use pallet_evm::{
	Account as EVMAccount, EVMCurrencyAdapter, EnsureAddressNever, EnsureAddressRoot,
	FeeCalculator, GasWeightMapping, IdentityAddressMapping, Runner,
};
use pallet_grandpa::{
	fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList,
};
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_session::historical::{self as pallet_session_historical};
pub use pallet_timestamp::Call as TimestampCall;
use pallet_transaction_payment::CurrencyAdapter;

pub use frame_support::{
	construct_runtime,
	dispatch::{DispatchClass, GetDispatchInfo},
	pallet_prelude::Get,
	parameter_types,
	traits::{
		ConstU128, ConstU32, ConstU8, Contains, Currency, EitherOfDiverse, EqualPrivilegeOnly,
		FindAuthor, Imbalance, KeyOwnerProofSystem, LockIdentifier, NeverEnsureOrigin, OnFinalize,
		OnUnbalanced, Randomness, StorageInfo, U128CurrencyToVote,
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
use frame_system::{EnsureRoot, EnsureSigned};

mod precompiles;
pub use precompiles::BifrostPrecompiles;

pub type Precompiles = BifrostPrecompiles<Runtime>;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	fp_self_contained::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

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

	/// Opaque block type.
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
	spec_name: create_runtime_str!("thebifrost-testnet"),
	// The name of the implementation of the spec.
	impl_name: create_runtime_str!("bifrost-testnet"),
	// The version of the authorship interface.
	authoring_version: 1,
	// The version of the runtime spec.
	spec_version: 462,
	// The version of the implementation of the spec.
	impl_version: 1,
	// A list of supported runtime APIs along with their versions.
	apis: RUNTIME_API_VERSIONS,
	// The version of the interface for handling transactions.
	transaction_version: 1,
	// The version of the interface for handling state transitions.
	state_version: 1,
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
	/// We allow for 5 MB blocks.
	pub BlockLength: frame_system::limits::BlockLength = frame_system::limits::BlockLength
		::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

/// The System pallet defines the core data types used in a Substrate runtime
impl frame_system::Config for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = frame_support::traits::Everything;
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = BlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = BlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type RuntimeCall = RuntimeCall;
	/// The lookup mechanism to get the account ID from whatever is passed in dispatchers.
	type Lookup = IdentityLookup<AccountId>;
	/// The index type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	/// The ubiquitous origin type.
	type RuntimeOrigin = RuntimeOrigin;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// Provides information about the pallet setup in the runtime.
	type PalletInfo = PalletInfo;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = ();
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = ();
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	/// The set code logic, just the default since we're not a parachain.
	type OnSetCode = ();
	/// The maximum number of consumers allowed on a single account.
	type MaxConsumers = ConstU32<16>;
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
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU128<0>;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type MaxHolds = ConstU32<1>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

pub struct DealWithFees<R>(sp_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for DealWithFees<R>
where
	R: pallet_balances::Config + pallet_treasury::Config,
	pallet_treasury::Pallet<R>: OnUnbalanced<NegativeImbalance<R>>,
{
	// this seems to be called for substrate-based transactions
	fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance<R>>) {
		if let Some(fees) = fees_then_tips.next() {
			// for fees, 50% are burned, 50% to the treasury
			let (_, to_treasury) = fees.ration(50, 50);
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
		let (_, to_treasury) = amount.ration(50, 50);
		<pallet_treasury::Pallet<R> as OnUnbalanced<_>>::on_unbalanced(to_treasury);
	}
}

parameter_types! {
	pub const TransactionByteFee: Balance = TRANSACTION_BYTE_FEE;
}

/// Provides the basic logic needed to pay the absolute minimum amount needed for a transaction to
/// be included.
impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = CurrencyAdapter<Balances, DealWithFees<Runtime>>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = ();
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
	type Preimages = Preimage;
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
	type SessionManager = BfcStaking;
	type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = opaque::SessionKeys;
	type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
}

impl pallet_session::historical::Config for Runtime {
	type FullIdentification = pallet_bfc_staking::ValidatorSnapshot<AccountId, Balance>;
	type FullIdentificationOf = pallet_bfc_staking::ValidatorSnapshotOf<Self>;
}

parameter_types! {
	pub const ImOnlineUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
	pub const MaxKeys: u32 = 10_000;
	pub const MaxPeerInHeartbeats: u32 = 10_000;
	pub const MaxPeerDataEncodingSize: u32 = 1_000;
	pub const DefaultSlashFraction: Perbill = Perbill::from_percent(10);
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type OverarchingCall = RuntimeCall;
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
	type MaxPeerDataEncodingSize = MaxPeerDataEncodingSize;
	type DefaultSlashFraction = DefaultSlashFraction;
}

/// The module that manages validator offences.
impl pallet_offences::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type IdentificationTuple = pallet_session::historical::IdentificationTuple<Self>;
	type OnOffenceHandler = BfcStaking;
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
	pub const CouncilMotionDuration: BlockNumber = 1 * DAYS;
	/// The maximum number of Proposals that can be open in the council at once.
	pub const CouncilMaxProposals: u32 = 100;
	/// The maximum number of council members.
	pub const CouncilMaxMembers: u32 = 100;

	/// The maximum amount of time (in blocks) for technical committee members to vote on motions.
	/// Motions may end in fewer blocks if enough votes are cast to determine the result.
	pub const TechCommitteeMotionDuration: BlockNumber = 1 * DAYS;
	/// The maximum number of Proposals that can be open in the technical committee at once.
	pub const TechCommitteeMaxProposals: u32 = 100;
	/// The maximum number of technical committee members.
	pub const TechCommitteeMaxMembers: u32 = 100;

	pub MaxProposalWeight: Weight = BlockWeights::get().max_block;
}

/// A type that represents a council member for governance
pub type CouncilInstance = pallet_collective::Instance1;

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
}

/// A type that represents a technical committee member for governance
pub type TechCommitteeInstance = pallet_collective::Instance2;

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

/// The purpose of this offset is to ensure that a democratic proposal will not apply in the same
/// block as a round change.
const ENACTMENT_OFFSET: u32 = 10;

parameter_types! {
	pub const LaunchPeriod: BlockNumber = 1 * DAYS;
	pub const VotingPeriod: BlockNumber = 1 * DAYS;
	pub const VoteLockingPeriod: BlockNumber = 1 * DAYS;
	pub const FastTrackVotingPeriod: BlockNumber = 1 * HOURS;
	pub const EnactmentPeriod: BlockNumber = 1 * DAYS + ENACTMENT_OFFSET;
	pub const CooloffPeriod: BlockNumber = 1 * DAYS;
	pub const MinimumDeposit: Balance = 5 * SUPPLY_FACTOR * BFC;
	pub const MaxVotes: u32 = 50;
	pub const MaxProposals: u32 = 50;
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
	pub const ProposalBondMinimum: Balance = 5 * BFC;
	pub const SpendPeriod: BlockNumber = 1 * DAYS;
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const MaxApprovals: u32 = 50;
	pub TreasuryAccount: AccountId = Treasury::account_id();
}

/// A module that manages funds stored in a certain vault
impl pallet_treasury::Config for Runtime {
	type PalletId = TreasuryPalletId;
	type Currency = Balances;
	type ApproveOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilInstance, 3, 5>,
	>;
	type RejectOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		pallet_collective::EnsureProportionMoreThan<AccountId, CouncilInstance, 1, 2>,
	>;
	type RuntimeEvent = RuntimeEvent;
	type OnSlash = Treasury;
	type ProposalBond = ProposalBond;
	type ProposalBondMinimum = ProposalBondMinimum;
	type ProposalBondMaximum = ();
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
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BenchmarkHelper;
}

parameter_types! {
	pub const BasicDeposit: Balance = 100 * BFC;
	pub const FieldDeposit: Balance = 100 * BFC;
	pub const SubAccountDeposit: Balance = 100 * BFC;
	pub const MaxSubAccounts: u32 = 100;
	pub const MaxAdditionalFields: u32 = 100;
	pub const MaxRegistrars: u32 = 20;
}

/// The module that manages account identities and registrar judgements.
impl pallet_identity::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BasicDeposit = BasicDeposit;
	type FieldDeposit = FieldDeposit;
	type SubAccountDeposit = SubAccountDeposit;
	type MaxSubAccounts = MaxSubAccounts;
	type MaxAdditionalFields = MaxAdditionalFields;
	type IdentityInformation = IdentityInfo<MaxAdditionalFields>;
	type MaxRegistrars = MaxRegistrars;
	type Slashed = Treasury;
	type ForceOrigin = EnsureRoot<AccountId>;
	type RegistrarOrigin = EnsureRoot<AccountId>;
	type WeightInfo = pallet_identity::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const DefaultOffenceExpirationInSessions: u32 = 1u32;
	pub const DefaultFullMaximumOffenceCount: u32 = 5u32;
	pub const DefaultBasicMaximumOffenceCount: u32 = 3u32;
	pub const IsOffenceActive: bool = true;
	pub const IsSlashActive: bool = true;
}

/// A module that wraps `pallet_offences` to act as a central offence handler
impl pallet_bfc_offences::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
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
	pub const DefaultHeartbeatSlashFraction: Perbill = Perbill::from_percent(20);
}

/// A module that manages registered relayers for cross chain interoperability
impl pallet_relay_manager::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorSet = Historical;
	type ReportUnresponsiveness = Offences;
	type StorageCacheLifetimeInRounds = StorageCacheLifetimeInRounds;
	type IsHeartbeatOffenceActive = IsHeartbeatOffenceActive;
	type DefaultHeartbeatSlashFraction = DefaultHeartbeatSlashFraction;
	type WeightInfo = pallet_relay_manager::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	/// Minimum round length is 30 seconds (10 * 3 second block times).
	pub const MinBlocksPerRound: u32 = 10;
	/// Blocks per round.
	pub const DefaultBlocksPerRound: u32 = 8 * HOURS;
	/// Rounds before the validator leaving the candidates request can be executed.
	pub const LeaveCandidatesDelay: u32 = 2;
	/// Rounds before the candidate bond increase/decrease can be executed.
	pub const CandidateBondLessDelay: u32 = 2;
	/// Rounds before the nominator exit can be executed.
	pub const LeaveNominatorsDelay: u32 = 2;
	/// Rounds before the nominator revocation can be executed.
	pub const RevokeNominationDelay: u32 = 2;
	/// Rounds before the nominator bond increase/decrease can be executed.
	pub const NominationBondLessDelay: u32 = 2;
	/// Rounds before the reward is paid.
	pub const RewardPaymentDelay: u32 = 1;
	/// Default maximum full validators selected per round, default at genesis.
	pub const DefaultMaxSelectedFullCandidates: u32 = 30;
	/// Default maximum basic validators selected per round, default at genesis.
	pub const DefaultMaxSelectedBasicCandidates: u32 = 170;
	/// Maximum top nominations per candidate.
	pub const MaxTopNominationsPerCandidate: u32 = 100;
	/// Maximum bottom nominations per candidate.
	pub const MaxBottomNominationsPerCandidate: u32 = 50;
	/// Maximum nominations per nominator.
	pub const MaxNominationsPerNominator: u32 = 10;
	/// Default commission rate for full validators.
	pub const DefaultFullValidatorCommission: Perbill = Perbill::from_percent(50);
	/// Default commission rate for basic validators.
	pub const DefaultBasicValidatorCommission: Perbill = Perbill::from_percent(10);
	/// Maximum commission rate available for full validators.
	pub const MaxFullValidatorCommission: Perbill = Perbill::from_percent(100);
	/// Maximum commission rate available for basic validators.
	pub const MaxBasicValidatorCommission: Perbill = Perbill::from_percent(20);
	/// Minimum stake required to become a full validator.
	pub const MinFullValidatorStk: u128 = 100_000 * SUPPLY_FACTOR * BFC;
	/// Minimum stake required to become a basic validator.
	pub const MinBasicValidatorStk: u128 = 50_000 * SUPPLY_FACTOR * BFC;
	/// Minimum stake required to be reserved to be a full candidate.
	pub const MinFullCandidateStk: u128 = 100_000 * SUPPLY_FACTOR * BFC;
	/// Minimum stake required to be reserved to be a basic candidate.
	pub const MinBasicCandidateStk: u128 = 50_000 * SUPPLY_FACTOR * BFC;
	/// Minimum stake required to be reserved to be a nominator.
	pub const MinNominatorStk: u128 = 1_000 * SUPPLY_FACTOR * BFC;
}

/// Minimal staking pallet that implements validator selection by total backed stake.
impl pallet_bfc_staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type MonetaryGovernanceOrigin = EnsureRoot<AccountId>;
	type RelayManager = RelayManager;
	type OffenceHandler = BfcOffences;
	type MinBlocksPerRound = MinBlocksPerRound;
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
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type MintableOrigin =
		pallet_collective::EnsureProportionMoreThan<AccountId, CouncilInstance, 1, 2>;
	type WeightInfo = pallet_bfc_utility::weights::SubstrateWeight<Runtime>;
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
}

pub struct FindAuthorAccountId<F>(sp_std::marker::PhantomData<F>);
impl<F: FindAuthor<u32>> FindAuthor<H160> for FindAuthorAccountId<F> {
	fn find_author<'a, I>(digests: I) -> Option<H160>
	where
		I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
	{
		if let Some(author_index) = F::find_author(digests) {
			let authority_id = Aura::authorities()[author_index as usize].clone();
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
		UncheckedExtrinsic::new_unsigned(
			pallet_ethereum::Call::<Runtime>::transact { transaction }.into(),
		)
	}
}
impl fp_rpc::ConvertTransaction<opaque::UncheckedExtrinsic> for TransactionConverter {
	fn convert_transaction(
		&self,
		transaction: pallet_ethereum::Transaction,
	) -> opaque::UncheckedExtrinsic {
		let extrinsic = UncheckedExtrinsic::new_unsigned(
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
	type RuntimeEvent = RuntimeEvent;
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
	type Timestamp = Timestamp;
	type WeightInfo = pallet_evm::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const PostBlockAndTxnHashes: PostLogContent = PostLogContent::BlockAndTxnHashes;
}

/// The Ethereum module is responsible for storing block data and provides RPC compatibility.
impl pallet_ethereum::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type StateRoot = pallet_ethereum::IntermediateStateRoot<Self>;
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
	type RuntimeEvent = RuntimeEvent;
	type Threshold = BaseFeeThreshold;
	type DefaultBaseFeePerGas = DefaultBaseFeePerGas;
	type DefaultElasticity = DefaultElasticity;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
	pub struct Runtime {
		// System
		System: frame_system::{Pallet, Call, Storage, Config<T>, Event<T>} = 0,
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent} = 2,

		// Block
		Aura: pallet_aura::{Pallet, Storage, Config<T>} = 3,

		// Consensus
		Authorship: pallet_authorship::{Pallet, Storage} = 4,
		Session: pallet_session::{Pallet, Call, Storage, Config<T>, Event} = 5,
		Historical: pallet_session_historical::{Pallet} = 6,
		Offences: pallet_offences::{Pallet, Storage, Event} = 7,
		ImOnline: pallet_im_online::{Pallet, Call, Storage, ValidateUnsigned, Config<T>, Event<T>} = 8,
		Grandpa: pallet_grandpa::{Pallet, Call, Storage, ValidateUnsigned, Config<T>, Event} = 9,

		// Monetary
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 10,
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Config<T>, Event<T>} = 11,

		// Staking
		RelayManager: pallet_relay_manager::{Pallet, Call, Storage, Config<T>, Event<T>} = 20,
		BfcStaking: pallet_bfc_staking::{Pallet, Call, Storage, Config<T>, Event<T>} = 21,
		BfcUtility: pallet_bfc_utility::{Pallet, Call, Storage, Config<T>, Event<T>} = 22,
		BfcOffences: pallet_bfc_offences::{Pallet, Call, Storage, Config<T>, Event<T>} = 23,

		// Utility
		Utility: pallet_utility::{Pallet, Call, Event} = 30,
		Identity: pallet_identity::{Pallet, Call, Storage, Event<T>} = 31,

		// Ethereum
		EVM: pallet_evm::{Pallet, Config<T>, Call, Storage, Event<T>} = 40,
		Ethereum: pallet_ethereum::{Pallet, Call, Storage, Event, Origin, Config<T>} = 41,
		BaseFee: pallet_base_fee::{Pallet, Call, Storage, Config<T>, Event} = 42,

		// Governance
		Scheduler: pallet_scheduler::{Pallet, Storage, Event<T>, Call} = 50,
		Democracy: pallet_democracy::{Pallet, Storage, Config<T>, Event<T>, Call} = 51,
		Council: pallet_collective::<Instance1>::{Pallet, Call, Storage, Origin<T>, Event<T>, Config<T>} = 52,
		TechnicalCommittee: pallet_collective::<Instance2>::{Pallet, Call, Storage, Origin<T>, Event<T>, Config<T>} = 53,
		CouncilMembership: pallet_membership::<Instance1>::{Pallet, Call, Storage, Event<T>, Config<T>} = 54,
		TechnicalMembership: pallet_membership::<Instance2>::{Pallet, Call, Storage, Event<T>, Config<T>} = 55,
		Treasury: pallet_treasury::{Pallet, Call, Storage, Config<T>, Event<T>} = 56,
		Preimage: pallet_preimage::{Pallet, Call, Storage, Event<T>, HoldReason} = 57,

		// Temporary
		Sudo: pallet_sudo::{Pallet, Call, Storage, Config<T>, Event<T>} = 99,
	}
);

bifrost_common_runtime::impl_common_runtime_apis!();
bifrost_common_runtime::impl_self_contained_call!();
