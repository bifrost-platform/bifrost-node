#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_btc_registration_pool::{Call as BtcRegistrationPoolCall, PoolRound};
use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use bp_btc_relay::{AddressState, BoundedBitcoinAddress, MigrationSequence};
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
	#[precompile::public("currentRound()")]
	#[precompile::public("current_round()")]
	#[precompile::view]
	fn current_round(handle: &mut impl PrecompileHandle) -> EvmResult<PoolRound> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		Ok(Self::get_current_round())
	}

	#[precompile::public("registrationInfo(address,uint32)")]
	#[precompile::public("registration_info(address,uint32)")]
	#[precompile::view]
	fn registration_info(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
		pool_round: PoolRound,
	) -> EvmResult<EvmRegistrationInfoOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut info = RegistrationInfo::default();
		let target_round = Self::target_round(pool_round);

		if user_bfc_address == Address(handle.context().address) {
			if let Some(system_vault) = BtcRegistrationPoolOf::<Runtime>::system_vault(target_round)
			{
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
				BtcRegistrationPoolOf::<Runtime>::registration_pool(target_round, &user_bfc_address)
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

	#[precompile::public("registrationPool(uint32)")]
	#[precompile::public("registration_pool(uint32)")]
	#[precompile::view]
	fn registration_pool(
		handle: &mut impl PrecompileHandle,
		pool_round: PoolRound,
	) -> EvmResult<EvmRegistrationPoolOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut user_bfc_addresses: Vec<Address> = vec![];
		let mut refund_addresses: Vec<BitcoinAddressString> = vec![];
		let mut vault_addresses: Vec<BitcoinAddressString> = vec![];

		let target_round = Self::target_round(pool_round);
		pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter_prefix(target_round)
			.for_each(|(bfc_address, relay_target)| {
				user_bfc_addresses.push(Address(bfc_address.into()));
				refund_addresses
					.push(BitcoinAddressString::from(relay_target.refund_address.into_inner()));

				let vault_address = match relay_target.vault.address {
					AddressState::Pending => BoundedVec::default(),
					AddressState::Generated(address) => address,
				};
				vault_addresses.push(BitcoinAddressString::from(vault_address.into_inner()));
			});
		Ok((user_bfc_addresses, refund_addresses, vault_addresses))
	}

	#[precompile::public("pendingRegistrations(uint32)")]
	#[precompile::public("pending_registrations(uint32)")]
	#[precompile::view]
	fn pending_registrations(
		handle: &mut impl PrecompileHandle,
		pool_round: PoolRound,
	) -> EvmResult<EvmPendingRegistrationsOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let target_round = Self::target_round(pool_round);

		let mut user_bfc_addresses: Vec<Address> = vec![];
		let mut refund_addresses: Vec<BitcoinAddressString> = vec![];

		if let Some(system_vault) = BtcRegistrationPoolOf::<Runtime>::system_vault(target_round) {
			if matches!(system_vault.address, AddressState::Pending) {
				user_bfc_addresses.push(Address(handle.context().address));
				refund_addresses.push(BitcoinAddressString::from(vec![]));
			}
		}

		pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter_prefix(target_round)
			.for_each(|(bfc_address, relay_target)| {
				if matches!(relay_target.vault.address, AddressState::Pending) {
					user_bfc_addresses.push(Address(bfc_address.into()));
					refund_addresses
						.push(BitcoinAddressString::from(relay_target.refund_address.into_inner()));
				}
			});
		Ok((user_bfc_addresses, refund_addresses))
	}

	#[precompile::public("pendingRefund(address,uint32)")]
	#[precompile::public("pending_refund(address,uint32)")]
	#[precompile::view]
	fn pending_refund(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
		pool_round: PoolRound,
	) -> EvmResult<BitcoinAddressString> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

		let target_round = Self::target_round(pool_round);
		let mut result = BitcoinAddressString::from(vec![]);
		if let Some(pending) = pallet_btc_registration_pool::PendingSetRefunds::<Runtime>::get(
			target_round,
			user_bfc_address,
		) {
			result = BitcoinAddressString::from(pending.new.into_inner());
		}
		Ok(result)
	}

	#[precompile::public("pendingRefunds(uint32)")]
	#[precompile::public("pending_refunds(uint32)")]
	#[precompile::view]
	fn pending_refunds(
		handle: &mut impl PrecompileHandle,
		pool_round: PoolRound,
	) -> EvmResult<Vec<(Address, BitcoinAddressString, BitcoinAddressString)>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let target_round = Self::target_round(pool_round);

		Ok(pallet_btc_registration_pool::PendingSetRefunds::<Runtime>::iter_prefix(target_round)
			.map(|(who, pending)| {
				(
					Address(who.into()),
					BitcoinAddressString::from(pending.old.into_inner()),
					BitcoinAddressString::from(pending.new.into_inner()),
				)
			})
			.collect())
	}

	#[precompile::public("vaultAddresses(uint32)")]
	#[precompile::public("vault_addresses(uint32)")]
	#[precompile::view]
	fn vault_addresses(
		handle: &mut impl PrecompileHandle,
		pool_round: PoolRound,
	) -> EvmResult<Vec<BitcoinAddressString>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut vault_addresses: Vec<BitcoinAddressString> =
			pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter_prefix(
				Self::target_round(pool_round),
			)
			.filter_map(|(_, relay_target)| match relay_target.vault.address {
				AddressState::Pending => None,
				AddressState::Generated(address) => {
					Some(BitcoinAddressString::from(address.into_inner()))
				},
			})
			.collect();
		// add system vault if it exists
		if let Some(system_vault) =
			BtcRegistrationPoolOf::<Runtime>::system_vault(Self::target_round(pool_round))
		{
			match system_vault.address {
				AddressState::Pending => (),
				AddressState::Generated(address) => {
					vault_addresses.push(BitcoinAddressString::from(address.into_inner()));
				},
			}
		}

		Ok(vault_addresses)
	}

	#[precompile::public("vaultAddress(address,uint32)")]
	#[precompile::public("vault_address(address,uint32)")]
	#[precompile::view]
	fn vault_address(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
		pool_round: PoolRound,
	) -> EvmResult<BitcoinAddressString> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let target_round = Self::target_round(pool_round);

		let mut vault_address = BitcoinAddressString::from(vec![]);
		if user_bfc_address == Address(handle.context().address) {
			if let Some(system_vault) = BtcRegistrationPoolOf::<Runtime>::system_vault(target_round)
			{
				match system_vault.address {
					AddressState::Pending => (),
					AddressState::Generated(address) => {
						vault_address = BitcoinAddressString::from(address.into_inner());
					},
				}
			}
		} else {
			let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

			match BtcRegistrationPoolOf::<Runtime>::registration_pool(
				target_round,
				user_bfc_address,
			) {
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

	#[precompile::public("refundAddresses(uint32)")]
	#[precompile::public("refund_addresses(uint32)")]
	#[precompile::view]
	fn refund_addresses(
		handle: &mut impl PrecompileHandle,
		pool_round: PoolRound,
	) -> EvmResult<Vec<BitcoinAddressString>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let refund_addresses: Vec<BitcoinAddressString> =
			pallet_btc_registration_pool::BondedRefund::<Runtime>::iter_prefix(Self::target_round(
				pool_round,
			))
			.map(|(address, _)| BitcoinAddressString::from(address.into_inner()))
			.collect();
		Ok(refund_addresses)
	}

	#[precompile::public("refundAddress(address,uint32)")]
	#[precompile::public("refund_address(address,uint32)")]
	#[precompile::view]
	fn refund_address(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
		pool_round: PoolRound,
	) -> EvmResult<BitcoinAddressString> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

		let refund_address = match BtcRegistrationPoolOf::<Runtime>::registration_pool(
			Self::target_round(pool_round),
			user_bfc_address,
		) {
			Some(relay_target) => {
				BitcoinAddressString::from(relay_target.refund_address.into_inner())
			},
			None => BitcoinAddressString::from(vec![]),
		};
		Ok(refund_address)
	}

	#[precompile::public("userAddress(string,uint32)")]
	#[precompile::public("user_address(string,uint32)")]
	#[precompile::view]
	fn user_address(
		handle: &mut impl PrecompileHandle,
		vault_address: BitcoinAddressString,
		pool_round: PoolRound,
	) -> EvmResult<Address> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let vault_address =
			Self::convert_string_to_bitcoin_address(vault_address).in_field("vault_address")?;

		Ok(
			match BtcRegistrationPoolOf::<Runtime>::bonded_vault(
				Self::target_round(pool_round),
				vault_address,
			) {
				Some(address) => Address(address.into()),
				None => Address::default(),
			},
		)
	}

	#[precompile::public("descriptors(uint32)")]
	#[precompile::view]
	fn descriptors(
		handle: &mut impl PrecompileHandle,
		pool_round: PoolRound,
	) -> EvmResult<Vec<UnboundedString>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let descriptors: Vec<UnboundedString> = pallet_btc_registration_pool::BondedDescriptor::<
			Runtime,
		>::iter_prefix(Self::target_round(pool_round))
		.map(|(_, desc)| UnboundedString::from(desc))
		.collect();
		Ok(descriptors)
	}

	#[precompile::public("descriptor(string,uint32)")]
	#[precompile::view]
	fn descriptor(
		handle: &mut impl PrecompileHandle,
		vault_address: BitcoinAddressString,
		pool_round: PoolRound,
	) -> EvmResult<UnboundedString> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let vault_address =
			Self::convert_string_to_bitcoin_address(vault_address).in_field("vault_address")?;

		Ok(
			match BtcRegistrationPoolOf::<Runtime>::bonded_descriptor(
				Self::target_round(pool_round),
				vault_address,
			) {
				Some(desc) => UnboundedString::from(desc),
				None => UnboundedString::from(vec![]),
			},
		)
	}

	#[precompile::public("request_vault(string)")]
	#[precompile::public("requestVault(string)")]
	fn request_vault(
		handle: &mut impl PrecompileHandle,
		refund_address: BitcoinAddressString,
	) -> EvmResult {
		if BtcRegistrationPoolOf::<Runtime>::service_state() != MigrationSequence::Normal {
			return Err(RevertReason::custom("Service is under maintenance").into());
		}

		let caller = handle.context().caller;

		let raw_refund_address = refund_address.clone();
		let refund_address = Self::convert_string_to_bitcoin_address(raw_refund_address.clone())
			.in_field("refund_address")?;

		let call = BtcRegistrationPoolCall::<Runtime>::request_vault {
			refund_address: refund_address.to_vec(),
		};
		let origin = Runtime::AddressMapping::into_account_id(caller);
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_VAULT_PENDING,
			solidity::encode_event_data((Address(caller), raw_refund_address)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	#[precompile::public("request_set_refund(string)")]
	#[precompile::public("requestSetRefund(string)")]
	fn request_set_refund(
		handle: &mut impl PrecompileHandle,
		refund_address: BitcoinAddressString,
	) -> EvmResult {
		if BtcRegistrationPoolOf::<Runtime>::service_state() != MigrationSequence::Normal {
			return Err(RevertReason::custom("Service is under maintenance").into());
		}

		let caller = handle.context().caller;

		let raw_refund_address = refund_address.clone();
		let refund_address = Self::convert_string_to_bitcoin_address(raw_refund_address.clone())
			.in_field("refund_address")?;

		let call =
			BtcRegistrationPoolCall::<Runtime>::request_set_refund { new: refund_address.to_vec() };
		let origin = Runtime::AddressMapping::into_account_id(caller);
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	/// Converts a solidity string typed Bitcoin address to a `BoundedVec`.
	fn convert_string_to_bitcoin_address(
		string: BitcoinAddressString,
	) -> MayRevert<BoundedBitcoinAddress> {
		BoundedVec::try_from(string.as_bytes().to_vec())
			.map_err(|_| RevertReason::custom("invalid bytes").into())
	}

	/// Get current round of the BTC registration pool.
	fn get_current_round() -> PoolRound {
		BtcRegistrationPoolOf::<Runtime>::current_round()
	}

	/// Get the target round of the BTC registration pool.
	fn target_round(input: u32) -> PoolRound {
		if input == 0 {
			Self::get_current_round()
		} else {
			input
		}
	}
}
