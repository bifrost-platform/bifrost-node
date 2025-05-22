use bp_btc_relay::{
	traits::{PoolManager, SocketQueueManager},
	Address, AddressState, Descriptor, FromSliceError as KeyError, MigrationSequence,
	MultiSigAccount, Network, PublicKey, UnboundedBytes,
};
use frame_support::traits::SortedMembers;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::prelude::{
	format,
	string::{String, ToString},
};
use sp_core::{Get, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::{
	traits::{Block, Header, Verify},
	transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionValidity, ValidTransaction,
	},
	BoundedVec, DispatchError,
};
use sp_std::{fmt::Display, str, str::FromStr, vec::Vec};

use crate::{
	BoundedBitcoinAddress, Public, SetRefundsApproval, VaultKeyPreSubmission, VaultKeySubmission,
};

use super::pallet::*;

impl<T: Config> PoolManager<T::AccountId> for Pallet<T> {
	fn get_refund_address(who: &T::AccountId) -> Option<BoundedBitcoinAddress> {
		if let Some(relay_target) = RegistrationPool::<T>::get(CurrentRound::<T>::get(), who) {
			Some(relay_target.refund_address)
		} else {
			None
		}
	}

	fn get_vault_address(who: &T::AccountId) -> Option<BoundedBitcoinAddress> {
		if let Some(relay_target) = RegistrationPool::<T>::get(CurrentRound::<T>::get(), who) {
			match relay_target.vault.address {
				AddressState::Pending => None,
				AddressState::Generated(address) => Some(address),
			}
		} else {
			None
		}
	}

	fn get_bonded_descriptor(who: &BoundedBitcoinAddress) -> Option<Descriptor<PublicKey>> {
		if let Some(descriptor) = <BondedDescriptor<T>>::get(CurrentRound::<T>::get(), who) {
			let descriptor_str = match str::from_utf8(&descriptor) {
				Ok(str) => str,
				Err(_) => return None,
			};
			match Descriptor::<PublicKey>::from_str(descriptor_str) {
				Ok(descriptor) => Some(descriptor),
				Err(_) => None,
			}
		} else {
			None
		}
	}

	fn get_system_vault(round: u32) -> Option<BoundedBitcoinAddress> {
		if let Some(vault) = SystemVault::<T>::get(round) {
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
		ServiceState::<T>::get()
	}

	fn get_current_round() -> u32 {
		CurrentRound::<T>::get()
	}

	fn add_migration_tx(txid: H256) {
		<OngoingVaultMigration<T>>::mutate(|states| {
			if states.get(&txid).is_none() {
				states.insert(txid, false);
			}
		});
	}

	fn remove_migration_tx(txid: H256) {
		<OngoingVaultMigration<T>>::mutate(|states| {
			states.remove(&txid);
		});
	}

	fn execute_migration_tx(txid: H256) {
		<OngoingVaultMigration<T>>::mutate(|states| {
			if states.get(&txid).is_some() {
				states.insert(txid, true);
			}
		});
	}

	fn replace_authority(old: &T::AccountId, new: &T::AccountId) {
		let round = CurrentRound::<T>::get();
		// move pre-submitted pub keys from old to new
		let pre_submitted_pub_keys = <PreSubmittedPubKeys<T>>::take(round, old);
		if !pre_submitted_pub_keys.is_empty() {
			<PreSubmittedPubKeys<T>>::insert(round, new, pre_submitted_pub_keys);
		}
		// replace authority in system vault (if it's pending)
		if let Some(mut vault) = SystemVault::<T>::get(round) {
			if vault.address == AddressState::Pending {
				vault.replace_authority(old, new);
			}
		}
		// replace authority in all registration pools (if they are pending)
		<RegistrationPool<T>>::iter_prefix(round).for_each(|(_, mut relay_target)| {
			if relay_target.vault.address == AddressState::Pending {
				relay_target.vault.replace_authority(old, new);
			}
		});
	}

	fn get_pending_set_refunds() -> Vec<(T::AccountId, BoundedBitcoinAddress)> {
		<PendingSetRefunds<T>>::iter_prefix(CurrentRound::<T>::get())
			.map(|(who, state)| (who, state.new))
			.collect::<Vec<_>>()
	}

	fn try_approve_set_refund(
		who: &T::AccountId,
		new: &BoundedBitcoinAddress,
	) -> Result<(), DispatchError> {
		let round = CurrentRound::<T>::get();
		// check if the new refund address is already bonded as a vault
		// if it is, then we just remove the pending refund set and do nothing
		if !<BondedVault<T>>::contains_key(round, new) {
			let mut relay_target =
				<RegistrationPool<T>>::get(round, who).ok_or(Error::<T>::UserDNE)?;
			// remove from previous bond
			let old = relay_target.refund_address.clone();
			<BondedRefund<T>>::mutate(round, &old, |users| {
				users.retain(|u| *u != *who);
			});
			// add to new bond
			<BondedRefund<T>>::mutate(round, new, |users| {
				users.push(who.clone());
			});

			relay_target.set_refund_address(new.clone());
			<RegistrationPool<T>>::insert(round, who, relay_target);

			Self::deposit_event(Event::RefundSetApproved {
				who: who.clone(),
				old,
				new: new.clone(),
			});
		}
		<PendingSetRefunds<T>>::remove(round, who);
		Ok(())
	}
}

impl<T: Config> Pallet<T> {
	/// Get the `m` value.
	pub fn get_m() -> u32 {
		MultiSigRatio::<T>::get().mul_ceil(Self::get_n())
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

	/// Tries to generate a vault address with the given public keys.
	/// If the generated address is already used as a refund address, the stored public keys will be cleared.
	/// If not, the address will be bonded successfully.
	pub fn try_bond_vault_address(
		vault: &mut MultiSigAccount<T::AccountId>,
		refund_address: &BoundedBitcoinAddress,
		who: T::AccountId,
		current_round: u32,
	) -> Result<(), DispatchError> {
		// generate vault address
		let (vault_address, descriptor) = Self::generate_vault_address(vault.pub_keys())?;

		// check if address is already in used as a refund address
		if <BondedRefund<T>>::contains_key(current_round, &vault_address) {
			return Err(Error::<T>::AddressAlreadyRegistered.into());
		} else {
			vault.set_address(vault_address.clone());
			vault.set_descriptor(descriptor.clone());

			<BondedVault<T>>::insert(current_round, &vault_address, who.clone());
			<BondedDescriptor<T>>::insert(current_round, &vault_address, descriptor);

			Self::deposit_event(Event::VaultGenerated {
				who: who.clone(),
				refund_address: refund_address.clone(),
				vault_address,
			});
		}
		Ok(())
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

	/// Verify the key submission signature.
	pub fn verify_key_submission(
		key_submission: &VaultKeySubmission<T::AccountId>,
		signature: &T::Signature,
		tag_prefix: &'static str,
	) -> TransactionValidity {
		let VaultKeySubmission { authority_id, who, pub_key, pool_round } = key_submission;

		// verify if the authority is a relay executive member.
		if !T::Executives::contains(authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}

		// verify if the signature was originated from the authority.
		let message = format!("{}:{}", pool_round, array_bytes::bytes2hex("0x", pub_key));
		if !signature.verify(message.as_bytes(), authority_id) {
			return Err(InvalidTransaction::BadProof.into());
		}

		ValidTransaction::with_tag_prefix(tag_prefix)
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, who))
			.propagate(true)
			.build()
	}

	/// Verify the key pre-submission signature.
	pub fn verify_key_presubmission(
		vault_key_pre_submission: &VaultKeyPreSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let VaultKeyPreSubmission { authority_id, pub_keys, pool_round } = vault_key_pre_submission;

		// verify if the authority is a relay executive member.
		if !T::Executives::contains(&authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}

		// verify if the signature was originated from the authority.
		let message = format!(
			"{}:{}",
			pool_round,
			pub_keys
				.iter()
				.map(|x| array_bytes::bytes2hex("0x", x))
				.collect::<Vec<String>>()
				.concat()
		);
		if !signature.verify(message.as_bytes(), &authority_id) {
			return Err(InvalidTransaction::BadProof.into());
		}

		ValidTransaction::with_tag_prefix("KeyPreSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, pub_keys))
			.propagate(true)
			.build()
	}

	/// Verifies the refund set approval signature.
	pub fn verify_set_refunds_approval(
		approval: &SetRefundsApproval<T::AccountId, BlockNumberFor<T>>,
		signature: &T::Signature,
	) -> TransactionValidity
	where
		<T as frame_system::Config>::AccountId: AsRef<[u8]>,
		<<<T as frame_system::Config>::Block as Block>::Header as Header>::Number: Display,
	{
		let SetRefundsApproval { authority_id, refund_sets, pool_round, deadline } = approval;

		// verify if the authority matches with the `SocketQueue::Authority`.
		T::SocketQueue::verify_authority(authority_id)?;

		// verify if the deadline is not expired.
		let now = <frame_system::Pallet<T>>::block_number();
		if now > *deadline {
			return Err(InvalidTransaction::Stale.into());
		}

		// verify if the signature was originated from the authority.
		let message = [
			keccak_256("SetRefundsApproval".as_bytes()).as_slice(),
			format!(
				"{}:{}:{}",
				pool_round,
				deadline,
				refund_sets
					.into_iter()
					.map(|x| hex::encode(x.0.clone()))
					.collect::<Vec<String>>()
					.concat()
			)
			.as_bytes(),
		]
		.concat();
		if !signature.verify(&*message, &authority_id) {
			return Err(InvalidTransaction::BadProof.into());
		}

		ValidTransaction::with_tag_prefix("SetRefundsApproval")
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, refund_sets))
			.propagate(true)
			.build()
	}
}
