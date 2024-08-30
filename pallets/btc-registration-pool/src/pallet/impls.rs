use bp_multi_sig::{
	traits::PoolManager, Address, AddressState, Descriptor, Error as KeyError, MigrationSequence,
	Network, PublicKey, UnboundedBytes,
};
use frame_support::traits::SortedMembers;
use scale_info::prelude::string::ToString;
use sp_core::Get;
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::{str, str::FromStr, vec::Vec};

use crate::{BoundedBitcoinAddress, Public};

use super::pallet::*;

impl<T: Config> PoolManager<T::AccountId> for Pallet<T> {
	fn get_refund_address(who: &T::AccountId) -> Option<BoundedBitcoinAddress> {
		if let Some(relay_target) = Self::registration_pool(Self::current_round(), who) {
			Some(relay_target.refund_address)
		} else {
			None
		}
	}

	fn get_vault_address(who: &T::AccountId) -> Option<BoundedBitcoinAddress> {
		if let Some(relay_target) = Self::registration_pool(Self::current_round(), who) {
			match relay_target.vault.address {
				AddressState::Pending => None,
				AddressState::Generated(address) => Some(address),
			}
		} else {
			None
		}
	}

	fn get_system_vault(round: u32) -> Option<BoundedBitcoinAddress> {
		if let Some(vault) = Self::system_vault(round) {
			match vault.address {
				AddressState::Pending => None,
				AddressState::Generated(address) => Some(address),
			}
		} else {
			None
		}
	}

	fn get_bitcoin_network() -> Network {
		T::BitcoinNetwork::get()
	}

	fn get_bitcoin_chain_id() -> u32 {
		T::BitcoinChainId::get()
	}

	fn get_service_state() -> MigrationSequence {
		Self::service_state()
	}

	fn get_current_round() -> u32 {
		Self::current_round()
	}
}

impl<T: Config> Pallet<T> {
	/// Get the `m` value.
	pub fn get_m() -> u32 {
		Self::m_n_ratio().mul_ceil(Self::get_n())
	}

	/// Get the `n` value.
	pub fn get_n() -> u32 {
		T::Executives::count() as u32
	}

	/// Convert string typed public keys to `PublicKey` type and return the sorted list.
	fn sort_pub_keys(raw_pub_keys: Vec<Public>) -> Result<Vec<PublicKey>, KeyError> {
		let mut pub_keys = raw_pub_keys
			.iter()
			.map(|raw_key| PublicKey::from_slice(raw_key.as_ref()))
			.collect::<Result<Vec<PublicKey>, _>>()?;
		pub_keys.sort();
		Ok(pub_keys)
	}

	/// Create a new wsh sorted multi descriptor.
	fn generate_descriptor(
		m: usize,
		raw_pub_keys: Vec<Public>,
	) -> Result<Descriptor<PublicKey>, ()> {
		let desc =
			Descriptor::new_wsh_sortedmulti(m, Self::sort_pub_keys(raw_pub_keys).map_err(|_| ())?)
				.map_err(|_| ())?;
		desc.sanity_check().map_err(|_| ())?;
		Ok(desc)
	}

	/// Generate a multi-sig vault address.
	pub fn generate_vault_address(
		raw_pub_keys: Vec<Public>,
	) -> Result<(BoundedBitcoinAddress, UnboundedBytes), DispatchError> {
		let desc = Self::generate_descriptor(Self::get_m() as usize, raw_pub_keys)
			.map_err(|_| Error::<T>::DescriptorGeneration)?;

		// generate vault address
		Ok((
			BoundedVec::try_from(
				desc.address(T::BitcoinNetwork::get())
					.map_err(|_| Error::<T>::DescriptorGeneration)?
					.to_string()
					.as_bytes()
					.to_vec(),
			)
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?,
			desc.to_string().as_bytes().to_vec(),
		))
	}

	/// Check if the given address is valid on the target Bitcoin network. Then returns the checked address.
	pub fn get_checked_bitcoin_address(
		address: &UnboundedBytes,
	) -> Result<BoundedBitcoinAddress, DispatchError> {
		let raw_address = str::from_utf8(address).map_err(|_| Error::<T>::InvalidBitcoinAddress)?;
		let unchecked_address =
			Address::from_str(raw_address).map_err(|_| Error::<T>::InvalidBitcoinAddress)?;
		let checked_address = unchecked_address
			.require_network(T::BitcoinNetwork::get())
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?
			.to_string();

		Ok(BoundedVec::try_from(checked_address.as_bytes().to_vec())
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?)
	}
}
