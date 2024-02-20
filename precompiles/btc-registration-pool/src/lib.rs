#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_btc_registration_pool::{BoundedBitcoinAddress, Call as BtcRegistrationPoolCall};
use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use fp_account::EthereumSignature;
use sp_core::{ecdsa::Signature, H160};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec, vec::Vec};

mod types;
use types::{BitcoinAddressBytes, BtcRegistrationPoolOf, EvmRegistrationPoolOf, SignatureBytes};

/// Solidity selector of the Registration log, which is the Keccak of the Log signature.
pub(crate) const SELECTOR_LOG_REGISTERED: [u8; 32] = keccak256!("Registered(address,bytes,bytes)");

/// A precompile to wrap the functionality from `pallet_btc_registration_pool`.
pub struct BtcRegistrationPoolPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> BtcRegistrationPoolPrecompile<Runtime>
where
	Runtime: pallet_btc_registration_pool::Config<Signature = EthereumSignature>
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::AccountId: Into<H160>,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<BtcRegistrationPoolCall<Runtime>>,
{
	#[precompile::public("registrationPool()")]
	#[precompile::public("registration_pool()")]
	#[precompile::view]
	fn registration_pool(handle: &mut impl PrecompileHandle) -> EvmResult<EvmRegistrationPoolOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut user_bfc_addresses: Vec<Address> = vec![];
		let mut refund_addresses: Vec<BitcoinAddressBytes> = vec![];
		let mut vault_addresses: Vec<BitcoinAddressBytes> = vec![];

		pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter().for_each(
			|(bfc_address, btc_pair)| {
				user_bfc_addresses.push(Address(bfc_address.into()));
				refund_addresses
					.push(BitcoinAddressBytes::from(btc_pair.refund_address.into_inner()));
				vault_addresses
					.push(BitcoinAddressBytes::from(btc_pair.vault_address.into_inner()));
			},
		);
		Ok((user_bfc_addresses, refund_addresses, vault_addresses))
	}

	#[precompile::public("vaultAddresses()")]
	#[precompile::public("vault_addresses()")]
	#[precompile::view]
	fn vault_addresses(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<BitcoinAddressBytes>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut vault_addresses: Vec<BitcoinAddressBytes> = vec![];
		pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter().for_each(
			|(_, btc_pair)| {
				vault_addresses
					.push(BitcoinAddressBytes::from(btc_pair.vault_address.into_inner()));
			},
		);
		Ok(vault_addresses)
	}

	#[precompile::public("vaultAddress(address)")]
	#[precompile::public("vault_address(address)")]
	#[precompile::view]
	fn vault_address(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
	) -> EvmResult<BitcoinAddressBytes> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

		let vault_address =
			match BtcRegistrationPoolOf::<Runtime>::registration_pool(user_bfc_address) {
				Some(pair) => BitcoinAddressBytes::from(pair.vault_address.into_inner()),
				None => BitcoinAddressBytes::from(vec![]),
			};
		Ok(vault_address)
	}

	#[precompile::public("refundAddresses()")]
	#[precompile::public("refund_addresses()")]
	#[precompile::view]
	fn refund_addresses(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<BitcoinAddressBytes>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut refund_addresses: Vec<BitcoinAddressBytes> = vec![];
		pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter().for_each(
			|(_, btc_pair)| {
				refund_addresses
					.push(BitcoinAddressBytes::from(btc_pair.refund_address.into_inner()));
			},
		);
		Ok(refund_addresses)
	}

	#[precompile::public("refundAddress(address)")]
	#[precompile::public("refund_address(address)")]
	#[precompile::view]
	fn refund_address(
		handle: &mut impl PrecompileHandle,
		user_bfc_address: Address,
	) -> EvmResult<BitcoinAddressBytes> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

		let refund_address =
			match BtcRegistrationPoolOf::<Runtime>::registration_pool(user_bfc_address) {
				Some(pair) => BitcoinAddressBytes::from(pair.refund_address.into_inner()),
				None => BitcoinAddressBytes::from(vec![]),
			};
		Ok(refund_address)
	}

	#[precompile::public("register(bytes,bytes,bytes)")]
	fn register(
		handle: &mut impl PrecompileHandle,
		refund_address: BitcoinAddressBytes,
		vault_address: BitcoinAddressBytes,
		signature: SignatureBytes,
	) -> EvmResult {
		let refund_address =
			Self::bytes_to_bitcoin_address(refund_address).in_field("refund_address")?;
		let vault_address =
			Self::bytes_to_bitcoin_address(vault_address).in_field("vault_address")?;
		let signature =
			EthereumSignature::new(Self::bytes_to_signature(signature).in_field("signature")?);

		let caller = handle.context().caller;
		let event = log1(
			handle.context().address,
			SELECTOR_LOG_REGISTERED,
			solidity::encode_event_data(Address(caller)),
		);
		handle.record_log_costs(&[&event])?;

		let call = BtcRegistrationPoolCall::<Runtime>::register {
			refund_address,
			vault_address,
			signature,
		};
		let origin = Runtime::AddressMapping::into_account_id(caller);
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		event.record(handle)?;

		Ok(())
	}

	fn bytes_to_bitcoin_address(bytes: BitcoinAddressBytes) -> MayRevert<BoundedBitcoinAddress> {
		BoundedVec::try_from(bytes.as_bytes().to_vec())
			.map_err(|_| RevertReason::custom("invalid bytes").into())
	}

	fn bytes_to_signature(bytes: SignatureBytes) -> MayRevert<Signature> {
		Signature::try_from(bytes.as_bytes())
			.map_err(|_| RevertReason::custom("invalid bytes").into())
	}
}
