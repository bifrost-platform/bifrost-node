mod impls;

use crate::{
	AttemptMetadata, BatchKeys, ChainAddress, ChainId, CloseOutcome, FlowId, HistoryPage, Instance,
	InstanceKey, InstanceMetadata, InstanceRef, InstanceScope, Lane, LaneDeclarations, LaneState,
	Part, ProductId, ProductModel, SlotId, SlotRecord, Template, TemplateId, TxHash, WeightInfo,
	MAX_DEFAULT_FLOWS, MAX_TEMPLATE_SLOTS,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion, BoundedBTreeMap};
use frame_system::pallet_prelude::*;
use sp_core::ConstU32;
use sp_std::marker::PhantomData;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Registers templates and assigns them to products.
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The single trusted recorder — wire as
		/// `pallet_tranche_tx_registry::EnsureTxRecorder` so every registry
		/// version shares one recorder identity.
		type RecorderOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		type WeightInfo: WeightInfo;

		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		// ---- templates / assignment ----
		/// A template with this `(name, version)` is already registered —
		/// templates are immutable, register a new version instead.
		TemplateExists,
		/// The template (or the one an instance is pinned to) isn't registered.
		UnknownTemplate,
		/// `main` is empty or has no required slot.
		NoRequiredMainSlot,
		/// A track has no required slot (its lanes could never be done).
		TrackWithoutRequiredSlot,
		/// A slot id appears twice in the template.
		DuplicateSlotId,
		/// Slot ids within `main` or a track aren't strictly ascending.
		SlotOrderNotAscending,
		/// A slot or track name is empty.
		EmptySlotName,
		/// A product model already has `MAX_DEFAULT_FLOWS` default flows.
		TooManyDefaultFlows,

		// ---- recording ----
		/// The product has no template assigned for this flow.
		FlowNotAssigned,
		/// The slot isn't in the instance's template.
		UnknownSlot,
		/// A main-track slot was recorded with a lane chain, or a track slot
		/// without one.
		LaneMismatch,
		/// Lane chain id `0` is reserved for "main".
		InvalidLaneChain,
		/// A lane declaration names a track the template doesn't have.
		UnknownTrack,
		/// The instance already has `MAX_INSTANCE_LANES` lanes.
		TooManyLanes,
		/// The instance doesn't exist and this attestation can't open it (only
		/// a main-track attestation opens an instance).
		InstanceNotOpened,
		/// The instance's template is investor-scoped and the opening
		/// attestation carries no investor.
		MissingInvestor,
		/// `scope` was given on the opening attestation of a template that is
		/// not investor-scoped — `LatestByScope` is keyed by investor.
		ScopeWithoutInvestor,
		/// `tx_hash` is empty or all zeros.
		ZeroTxHash,
		/// This `(chain_id, tx_hash, part)` is already attested to this slot —
		/// one tx may carry several parts (e.g. several FINALIZE chunks sent by
		/// one `continueFinalize`), each recorded once.
		DuplicateAttestation,
		/// The slot already holds `MAX_ATTEMPTS` attempts.
		TooManyAttempts,
		/// The instance already has `MAX_LINKS` links.
		TooManyLinks,
		/// A part's `count` is 0 or above `MAX_PART_COUNT`, or `index >= count`.
		InvalidPart,
		/// The attempt's part disagrees with the slot's earlier attempts — a
		/// different `count`, or a part on a single-part slot (or vice versa).
		PartCountMismatch,
		/// The instance is already completed or failed.
		InstanceAlreadyClosed,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A template was registered.
		TemplateRegistered { template: TemplateId },
		/// A product model's default flow was set (`Some`) or removed (`None`).
		DefaultFlowSet { model: ProductModel, flow_id: FlowId, template: Option<TemplateId> },
		/// A product's flow was assigned a template (`Some`) or unassigned.
		FlowAssigned { product_id: ProductId, flow_id: FlowId, template: Option<TemplateId> },
		/// The first main-track attestation for a key opened an instance.
		InstanceOpened {
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			template: TemplateId,
			investor: Option<ChainAddress>,
			scope: Option<InstanceScope>,
		},
		/// An instance gained a lane — explicitly declared or by recording into it.
		LaneDeclared {
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			track: u8,
			chain: ChainId,
		},
		/// One attestation was appended to a slot.
		TxRecorded {
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			lane: Lane,
			slot_id: SlotId,
			chain_id: ChainId,
			tx_hash: TxHash,
			completes: bool,
			part: Option<Part>,
		},
		/// An instance gained an outgoing link (indexed in reverse too).
		InstanceLinked {
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			target: InstanceRef,
		},
		/// An instance closed — by completion, or explicitly via `close_instance`.
		InstanceClosed {
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			outcome: CloseOutcome,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Registered templates — immutable once inserted.
	#[pallet::storage]
	pub type Templates<T: Config> = StorageMap<_, Blake2_128Concat, TemplateId, Template>;

	/// Flows (and their templates) a newly registered product receives, per
	/// product model.
	#[pallet::storage]
	pub type DefaultFlows<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ProductModel,
		BoundedBTreeMap<FlowId, TemplateId, ConstU32<MAX_DEFAULT_FLOWS>>,
		ValueQuery,
	>;

	/// The template new instances of `(product, flow)` open with.
	#[pallet::storage]
	pub type ProductFlows<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, ProductId, Blake2_128Concat, FlowId, TemplateId>;

	#[pallet::storage]
	pub type Instances<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
		),
		Instance<BlockNumberFor<T>>,
	>;

	/// Per-lane progress (sub lanes only — the main lane's lives on
	/// `Instance`).
	#[pallet::storage]
	pub type Lanes<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
			NMapKey<Blake2_128Concat, Lane>,
		),
		LaneState,
	>;

	/// Every attestation, per `(instance, lane, slot)`.
	#[pallet::storage]
	pub type Slots<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
			NMapKey<Blake2_128Concat, Lane>,
			NMapKey<Blake2_128Concat, SlotId>,
		),
		SlotRecord<BlockNumberFor<T>>,
		ValueQuery,
	>;

	/// Reverse link index — `(product, target)` → instances linking to it
	/// (a settlement → its requests), paged like investor history.
	#[pallet::storage]
	pub type LinkedFromLen<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
		),
		u32,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type LinkedFromPage<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
			NMapKey<Blake2_128Concat, u32>,
		),
		HistoryPage<InstanceRef>,
		ValueQuery,
	>;

	/// Open investor-scoped instances — one entry per instance, so opening
	/// and closing are O(1) regardless of how many an investor holds.
	#[pallet::storage]
	pub type InvestorActiveInstances<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ChainAddress>,
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
		),
		(),
	>;

	/// Number of `InvestorActiveInstances` entries per `(investor, product, flow)`.
	#[pallet::storage]
	pub type InvestorActiveCount<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ChainAddress>,
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
		),
		u32,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type InvestorHistoryLen<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ChainAddress>,
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
		),
		u32,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type InvestorHistoryPage<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ChainAddress>,
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, u32>,
		),
		HistoryPage<InstanceKey>,
		ValueQuery,
	>;

	/// Most recently opened instance per `(investor, product, flow, scope)`.
	/// Written once, when an instance opens with a `scope`; never cleared, so
	/// it also resolves closed instances (a whitelist action's latest nonce
	/// for a vault, an investor's latest request on a vault, ...).
	#[pallet::storage]
	pub type LatestByScope<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ChainAddress>,
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceScope>,
		),
		InstanceKey,
	>;

	// -----------------------------------------------------------------------
	// Genesis
	// -----------------------------------------------------------------------

	#[pallet::genesis_config]
	pub struct GenesisConfig<T> {
		/// Register the built-in default templates (`crate::defaults`) and
		/// the default flow maps.
		pub seed_default_templates: bool,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { seed_default_templates: true, _config: PhantomData }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if self.seed_default_templates {
				crate::defaults::seed::<T>();
			}
		}
	}

	// -----------------------------------------------------------------------
	// Calls
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register an immutable template under `id`.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::register_template(
			template.total_slots().min(MAX_TEMPLATE_SLOTS)
		))]
		pub fn register_template(
			origin: OriginFor<T>,
			id: TemplateId,
			template: Template,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::do_register_template(id, template)
		}

		/// Set (or, with `None`, remove) the template `flow_id` defaults to for
		/// newly registered products of `model`.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_default_flow())]
		pub fn set_default_flow(
			origin: OriginFor<T>,
			model: ProductModel,
			flow_id: FlowId,
			template: Option<TemplateId>,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::do_set_default_flow(model, flow_id, template)
		}

		/// Assign (or, with `None`, unassign) the template new instances of
		/// `(product_id, flow_id)` open with. Already open instances keep their
		/// own template.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::assign_flow())]
		pub fn assign_flow(
			origin: OriginFor<T>,
			product_id: ProductId,
			flow_id: FlowId,
			template: Option<TemplateId>,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			if let Some(template) = template {
				ensure!(Templates::<T>::contains_key(template), Error::<T>::UnknownTemplate);
			}
			ProductFlows::<T>::set(product_id, flow_id, template);
			Self::deposit_event(Event::FlowAssigned { product_id, flow_id, template });
			Ok(())
		}

		/// Give `product_id` every default flow of `model` it doesn't already
		/// have — for products registered before this pallet existed (new
		/// products get this automatically at registration).
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::assign_default_flows(MAX_DEFAULT_FLOWS))]
		pub fn assign_default_flows(
			origin: OriginFor<T>,
			product_id: ProductId,
			model: ProductModel,
		) -> DispatchResultWithPostInfo {
			T::GovernanceOrigin::ensure_origin(origin)?;
			let assigned = Self::do_assign_default_flows(product_id, model);
			Ok(Some(<T as Config>::WeightInfo::assign_default_flows(assigned)).into())
		}

		/// Attest one observed tx to one slot of one instance.
		///
		/// - `lane`: `None` for a main-track slot, `Some(chain)` for a track slot (the chain the
		///   step belongs to — not necessarily where `tx` ran; that's `chain_id`).
		/// - `completes`: whether this attestation succeeded (`false` for a `Reverted` bridge tx, a
		///   failed hook tx, an ignored message).
		/// - `part`: `(index, count)` for one part of a multi-part step (a settlement leg chunk,
		///   one of several payout moves from a chain); the slot completes once every index
		///   `0..count` has a completing attempt. `None` for a single-part step.
		/// - `investor`: required on the opening attestation of an investor-scoped instance,
		///   ignored otherwise.
		/// - `instance_metadata`: overwrites the instance's metadata if `Some`.
		/// - `declare_lanes`: `(track, chain)` lanes this instance has, added before the
		///   attestation is applied (e.g. a deposit's allocation chains at `RouteReceived`).
		///   Recording into an undeclared lane declares it implicitly.
		/// - `link`: another instance this one references (a request → its settlement); indexed
		///   both ways.
		///
		/// The first main-track attestation for an unseen `instance_key` opens
		/// the instance with the product's currently assigned template. A
		/// closed instance still accepts attestations as evidence, but its
		/// progress counters stay as they were at close.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::record_tx(
			declare_lanes.len() as u32,
			metadata.as_ref().map_or(0, |m| m.len() as u32)
				.saturating_add(instance_metadata.as_ref().map_or(0, |m| m.len() as u32)),
		))]
		pub fn record_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			lane: Option<ChainId>,
			slot_id: SlotId,
			chain_id: ChainId,
			tx_hash: TxHash,
			completes: bool,
			part: Option<Part>,
			metadata: Option<AttemptMetadata>,
			investor: Option<ChainAddress>,
			instance_metadata: Option<InstanceMetadata>,
			scope: Option<InstanceScope>,
			declare_lanes: LaneDeclarations,
			link: Option<InstanceRef>,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			Self::do_record_tx(
				product_id,
				flow_id,
				instance_key,
				lane,
				slot_id,
				chain_id,
				tx_hash,
				completes,
				part,
				metadata,
				investor,
				instance_metadata,
				scope,
				declare_lanes,
				link,
			)
		}

		/// Attest one tx to the same slot of many already open instances (one
		/// settlement tx approving/paying out N requests). Atomic — any
		/// failing instance reverts the whole call. A key listed twice is
		/// attested once.
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::record_tx(
			0,
			metadata.as_ref().map_or(0, |m| m.len() as u32),
		).saturating_mul(instance_keys.len() as u64))]
		pub fn record_tx_batch(
			origin: OriginFor<T>,
			product_id: ProductId,
			flow_id: FlowId,
			instance_keys: BatchKeys,
			lane: Option<ChainId>,
			slot_id: SlotId,
			chain_id: ChainId,
			tx_hash: TxHash,
			completes: bool,
			part: Option<Part>,
			metadata: Option<AttemptMetadata>,
			link: Option<InstanceRef>,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			let mut seen = sp_std::collections::btree_set::BTreeSet::new();
			for instance_key in instance_keys {
				if !seen.insert(instance_key) {
					continue;
				}
				ensure!(
					Instances::<T>::contains_key((product_id, flow_id, instance_key)),
					Error::<T>::InstanceNotOpened
				);
				Self::do_record_tx(
					product_id,
					flow_id,
					instance_key,
					lane,
					slot_id,
					chain_id,
					tx_hash.clone(),
					completes,
					part,
					metadata.clone(),
					None,
					None,
					None,
					Default::default(),
					link,
				)?;
			}
			Ok(())
		}

		/// Close an open instance explicitly — `Failed` for a flow that was
		/// abandoned (e.g. a request cleared on its spoke), `Completed` to
		/// finish one whose remaining steps will never be observed. Later
		/// attestations are still accepted as evidence. An instance that
		/// already closed (by completion or an earlier call) can't be re-closed.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::close_instance())]
		pub fn close_instance(
			origin: OriginFor<T>,
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			outcome: CloseOutcome,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			Self::do_close_instance(product_id, flow_id, instance_key, outcome)
		}
	}
}
