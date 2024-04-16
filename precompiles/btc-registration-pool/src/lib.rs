#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_btc_registration_pool::Call as BtcRegistrationPoolCall;
use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use bp_multi_sig::{AddressState, BoundedBitcoinAddress};
use fp_account::EthereumSignature;
use sp_core::H160;
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec, vec::Vec};

mod types;
use types::{
	BitcoinAddressString, EvmPendingRegistrationsOf, EvmRegistrationInfoOf, EvmRegistrationPoolOf,
	RegistrationInfo,
};

type BtcRegistrationPoolOf<Runtime> = pallet_btc_registration_pool::Pallet<Runtime>;

/// Solidity selector of the VaultPending log, which is the Keccak of the Log signature.
pub(crate) const SELECTOR_LOG_VAULT_PENDING: [u8; 32] = keccak256!("VaultPending(address,string)");

/// A precompile to wrap the functionality from `pallet_btc_registration_pool`.
pub struct BtcRegistrationPoolPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile]
impl<Runtime> BtcRegistrationPoolPrecompile<Runtime>
where
	Runtime: pallet_btc_registration_pool::Config<Signature = EthereumSignature>
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::AccountId: Into<H160> + From<H160>,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<BtcRegistrationPoolCall<Runtime>>,
{
	#[precompile::public("registrationInfo(address)")]
	#[precompile::public("registration_info(address)")]
	#[precompile::view]
	fn registration_info(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
	) -> EvmResult<EvmRegistrationInfoOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut info = RegistrationInfo::default();

		if user_bfc_address == Address(handle.context().address) {
			if let Some(system_vault) = BtcRegistrationPoolOf::<Runtime>::system_vault() {
				info.user_bfc_address = Address(handle.context().address);
				for (authority_id, pub_key) in system_vault.pub_keys.iter() {
					info.submitted_authorities.push(Address(authority_id.clone().into()));
					info.pub_keys.push(pub_key.0.into());
				}
				let vault_address = match system_vault.address {
					AddressState::Pending => BoundedVec::default(),
					AddressState::Generated(address) => address,
				};
				info.vault_address = BitcoinAddressString::from(vault_address.into_inner());
			}
		} else {
			let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);
			if let Some(relay_target) =
				BtcRegistrationPoolOf::<Runtime>::registration_pool(&user_bfc_address)
			{
				info.user_bfc_address = Address(user_bfc_address.into());
				info.refund_address =
					BitcoinAddressString::from(relay_target.refund_address.into_inner());

				for (authority_id, pub_key) in relay_target.vault.pub_keys.iter() {
					info.submitted_authorities.push(Address(authority_id.clone().into()));
					info.pub_keys.push(pub_key.0.into());
				}

				let vault_address = match relay_target.vault.address {
					AddressState::Pending => BoundedVec::default(),
					AddressState::Generated(address) => address,
				};
				info.vault_address = BitcoinAddressString::from(vault_address.into_inner());
			}
		}

		Ok(info.into())
	}

	#[precompile::public("registrationPool()")]
	#[precompile::public("registration_pool()")]
	#[precompile::view]
	fn registration_pool(handle: &mut impl PrecompileHandle) -> EvmResult<EvmRegistrationPoolOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut user_bfc_addresses: Vec<Address> = vec![];
		let mut refund_addresses: Vec<BitcoinAddressString> = vec![];
		let mut vault_addresses: Vec<BitcoinAddressString> = vec![];

		pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter().for_each(
			|(bfc_address, relay_target)| {
				user_bfc_addresses.push(Address(bfc_address.into()));
				refund_addresses
					.push(BitcoinAddressString::from(relay_target.refund_address.into_inner()));

				let vault_address = match relay_target.vault.address {
					AddressState::Pending => BoundedVec::default(),
					AddressState::Generated(address) => address,
				};
				vault_addresses.push(BitcoinAddressString::from(vault_address.into_inner()));
			},
		);
		Ok((user_bfc_addresses, refund_addresses, vault_addresses))
	}

	#[precompile::public("pendingRegistrations()")]
	#[precompile::public("pending_registrations()")]
	fn pending_registrations(
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<EvmPendingRegistrationsOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut user_bfc_addresses: Vec<Address> = vec![];
		let mut refund_addresses: Vec<BitcoinAddressString> = vec![];

		if let Some(system_vault) = BtcRegistrationPoolOf::<Runtime>::system_vault() {
			if matches!(system_vault.address, AddressState::Pending) {
				user_bfc_addresses.push(Address(handle.context().address));
				refund_addresses.push(BitcoinAddressString::from(vec![]));
			}
		}

		pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter().for_each(
			|(bfc_address, relay_target)| {
				if matches!(relay_target.vault.address, AddressState::Pending) {
					user_bfc_addresses.push(Address(bfc_address.into()));
					refund_addresses
						.push(BitcoinAddressString::from(relay_target.refund_address.into_inner()));
				}
			},
		);
		Ok((user_bfc_addresses, refund_addresses))
	}

	#[precompile::public("vaultAddresses()")]
	#[precompile::public("vault_addresses()")]
	#[precompile::view]
	fn vault_addresses(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<BitcoinAddressString>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let vault_addresses: Vec<BitcoinAddressString> =
			pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter()
				.filter_map(|(_, relay_target)| match relay_target.vault.address {
					AddressState::Pending => None,
					AddressState::Generated(address) => {
						Some(BitcoinAddressString::from(address.into_inner()))
					},
				})
				.collect();

		Ok(vault_addresses)
	}

	#[precompile::public("vaultAddress(address)")]
	#[precompile::public("vault_address(address)")]
	#[precompile::view]
	fn vault_address(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
	) -> EvmResult<BitcoinAddressString> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut vault_address = BitcoinAddressString::from(vec![]);
		if user_bfc_address == Address(handle.context().address) {
			if let Some(system_vault) = BtcRegistrationPoolOf::<Runtime>::system_vault() {
				match system_vault.address {
					AddressState::Pending => (),
					AddressState::Generated(address) => {
						vault_address = BitcoinAddressString::from(address.into_inner());
					},
				}
			}
		} else {
			let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

			match BtcRegistrationPoolOf::<Runtime>::registration_pool(user_bfc_address) {
				Some(btc_pair) => match btc_pair.vault.address {
					AddressState::Pending => (),
					AddressState::Generated(address) => {
						vault_address = BitcoinAddressString::from(address.into_inner())
					},
				},
				None => (),
			}
		}

		Ok(vault_address)
	}

	#[precompile::public("refundAddresses()")]
	#[precompile::public("refund_addresses()")]
	#[precompile::view]
	fn refund_addresses(
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<Vec<BitcoinAddressString>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let refund_addresses: Vec<BitcoinAddressString> =
			pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter()
				.map(|(_, relay_target)| {
					BitcoinAddressString::from(relay_target.refund_address.into_inner())
				})
				.collect();
		Ok(refund_addresses)
	}

	#[precompile::public("refundAddress(address)")]
	#[precompile::public("refund_address(address)")]
	#[precompile::view]
	fn refund_address(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
	) -> EvmResult<BitcoinAddressString> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

		let refund_address =
			match BtcRegistrationPoolOf::<Runtime>::registration_pool(user_bfc_address) {
				Some(relay_target) => {
					BitcoinAddressString::from(relay_target.refund_address.into_inner())
				},
				None => BitcoinAddressString::from(vec![]),
			};
		Ok(refund_address)
	}

	#[precompile::public("userAddress(string,bool)")]
	#[precompile::public("user_address(string,bool)")]
	#[precompile::view]
	fn user_address(
		handle: &mut impl PrecompileHandle,
		vault_or_refund: BitcoinAddressString,
		is_vault: bool,
	) -> EvmResult<Address> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let vault_or_refund =
			Self::convert_string_to_bitcoin_address(vault_or_refund).in_field("vault_or_refund")?;
		let user_address = if is_vault {
			BtcRegistrationPoolOf::<Runtime>::bonded_vault(vault_or_refund)
		} else {
			BtcRegistrationPoolOf::<Runtime>::bonded_refund(vault_or_refund)
		};

		Ok(match user_address {
			Some(address) => Address(address.into()),
			None => Address::default(),
		})
	}

	#[precompile::public("request_vault(string)")]
	#[precompile::public("requestVault(string)")]
	fn request_vault(
		handle: &mut impl PrecompileHandle,
		refund_address: BitcoinAddressString,
	) -> EvmResult {
		let caller = handle.context().caller;
		let event = log1(
			handle.context().address,
			SELECTOR_LOG_VAULT_PENDING,
			solidity::encode_event_data((Address(caller), refund_address.clone())),
		);
		handle.record_log_costs(&[&event])?;

		let refund_address =
			Self::convert_string_to_bitcoin_address(refund_address).in_field("refund_address")?;

		let call = BtcRegistrationPoolCall::<Runtime>::request_vault {
			refund_address: refund_address.to_vec(),
		};
		let origin = Runtime::AddressMapping::into_account_id(caller);
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		event.record(handle)?;

		Ok(())
	}

	/// Converts a solidity string typed Bitcoin address to a `BoundedVec`.
	fn convert_string_to_bitcoin_address(
		string: BitcoinAddressString,
	) -> MayRevert<BoundedBitcoinAddress> {
		BoundedVec::try_from(string.as_bytes().to_vec())
			.map_err(|_| RevertReason::custom("invalid bytes").into())
	}
}
