use pallet_relay_manager::{RelayerMetadata, RelayerStatus};

use precompile_utils::prelude::Address;

use sp_core::H160;
use sp_std::{marker::PhantomData, vec, vec::Vec};

pub type RelayManagerOf<Runtime> = pallet_relay_manager::Pallet<Runtime>;

pub type EvmRelayerStateOf = (Address, Address, u32);

pub type EvmRelayerStatesOf = (Vec<Address>, Vec<Address>, Vec<u32>);

/// EVM struct for relayer state
pub struct RelayerState<Runtime: pallet_relay_manager::Config> {
	/// This relayer's account
	pub relayer: Address,
	/// This relayer's controller account
	pub controller: Address,
	/// Current status of this relayer
	pub status: u32,
	/// Zero-sized type used to mark things that "act like" they own a T
	phantom: PhantomData<Runtime>,
}

impl<Runtime> RelayerState<Runtime>
where
	Runtime: pallet_relay_manager::Config,
	Runtime::AccountId: Into<H160>,
{
	pub fn default() -> Self {
		RelayerState {
			relayer: Address(Default::default()),
			controller: Address(Default::default()),
			status: 0u32,
			phantom: PhantomData,
		}
	}

	pub fn set_state(
		&mut self,
		relayer: Runtime::AccountId,
		state: RelayerMetadata<Runtime::AccountId>,
	) {
		self.relayer = Address(relayer.into());
		self.controller = Address(state.controller.into());
		self.status = match state.status {
			RelayerStatus::KickedOut => 2u32.into(),
			RelayerStatus::Active => 1u32.into(),
			RelayerStatus::Idle => 0u32.into(),
		};
	}
}

/// EVM struct for relayer states
pub struct RelayerStates<Runtime: pallet_relay_manager::Config> {
	/// This relayer's account
	pub relayer: Vec<Address>,
	/// This relayer's controller account
	pub controller: Vec<Address>,
	/// Current status of this relayer
	pub status: Vec<u32>,
	/// Zero-sized type used to mark things that "act like" they own a T
	phantom: PhantomData<Runtime>,
}

impl<Runtime> RelayerStates<Runtime>
where
	Runtime: pallet_relay_manager::Config,
	Runtime::AccountId: Into<H160>,
{
	pub fn default() -> Self {
		RelayerStates { relayer: vec![], controller: vec![], status: vec![], phantom: PhantomData }
	}

	pub fn insert_empty(&mut self) {
		self.relayer.push(Address(Default::default()));
		self.controller.push(Address(Default::default()));
		self.status.push(0u32);
	}

	pub fn insert_state(&mut self, state: RelayerState<Runtime>) {
		self.relayer.push(Address(state.relayer.into()));
		self.controller.push(Address(state.controller.into()));
		self.status.push(state.status);
	}
}

impl<Runtime> From<RelayerStates<Runtime>> for EvmRelayerStateOf
where
	Runtime: pallet_relay_manager::Config,
{
	fn from(state: RelayerStates<Runtime>) -> Self {
		(state.relayer[0], state.controller[0], state.status[0])
	}
}

impl<Runtime> From<RelayerStates<Runtime>> for EvmRelayerStatesOf
where
	Runtime: pallet_relay_manager::Config,
{
	fn from(states: RelayerStates<Runtime>) -> Self {
		(states.relayer, states.controller, states.status)
	}
}
