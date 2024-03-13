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
use types::{BitcoinAddressString, EvmRegistrationPoolOf, SignatureBytes};

type BtcRegistrationPoolOf<Runtime> = pallet_btc_registration_pool::Pallet<Runtime>;

/// Solidity selector of the Registration log, which is the Keccak of the Log signature.
pub(crate) const SELECTOR_LOG_REGISTERED: [u8; 32] =
	keccak256!("Registered(address,string,string)");

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
	// #[precompile::public("registrationPool()")]
	// #[precompile::public("registration_pool()")]
	// #[precompile::view]
	// fn registration_pool(handle: &mut impl PrecompileHandle) -> EvmResult<EvmRegistrationPoolOf> {
	// 	handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

	// 	let mut user_bfc_addresses: Vec<Address> = vec![];
	// 	let mut refund_addresses: Vec<BitcoinAddressString> = vec![];
	// 	let mut vault_addresses: Vec<BitcoinAddressString> = vec![];

	// 	pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter().for_each(
	// 		|(bfc_address, btc_pair)| {
	// 			user_bfc_addresses.push(Address(bfc_address.into()));
	// 			refund_addresses
	// 				.push(BitcoinAddressString::from(btc_pair.refund_address.into_inner()));
	// 			vault_addresses
	// 				.push(BitcoinAddressString::from(btc_pair.vault_address.into_inner()));
	// 		},
	// 	);
	// 	Ok((user_bfc_addresses, refund_addresses, vault_addresses))
	// }

	// #[precompile::public("vaultAddresses()")]
	// #[precompile::public("vault_addresses()")]
	// #[precompile::view]
	// fn vault_addresses(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<BitcoinAddressString>> {
	// 	handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

	// 	let mut vault_addresses: Vec<BitcoinAddressString> = vec![];
	// 	pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter().for_each(
	// 		|(_, btc_pair)| {
	// 			vault_addresses
	// 				.push(BitcoinAddressString::from(btc_pair.vault_address.into_inner()));
	// 		},
	// 	);
	// 	Ok(vault_addresses)
	// }

	// #[precompile::public("vaultAddress(address)")]
	// #[precompile::public("vault_address(address)")]
	// #[precompile::view]
	// fn vault_address(
	// 	handle: &mut impl PrecompileHandle,
	// 	user_bfc_address: Address,
	// ) -> EvmResult<BitcoinAddressString> {
	// 	handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
	// 	let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

	// 	let vault_address =
	// 		match BtcRegistrationPoolOf::<Runtime>::registration_pool(user_bfc_address) {
	// 			Some(pair) => BitcoinAddressString::from(pair.vault_address.into_inner()),
	// 			None => BitcoinAddressString::from(vec![]),
	// 		};
	// 	Ok(vault_address)
	// }

	// #[precompile::public("refundAddresses()")]
	// #[precompile::public("refund_addresses()")]
	// #[precompile::view]
	// fn refund_addresses(
	// 	handle: &mut impl PrecompileHandle,
	// ) -> EvmResult<Vec<BitcoinAddressString>> {
	// 	handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

	// 	let mut refund_addresses: Vec<BitcoinAddressString> = vec![];
	// 	pallet_btc_registration_pool::RegistrationPool::<Runtime>::iter().for_each(
	// 		|(_, btc_pair)| {
	// 			refund_addresses
	// 				.push(BitcoinAddressString::from(btc_pair.refund_address.into_inner()));
	// 		},
	// 	);
	// 	Ok(refund_addresses)
	// }

	// #[precompile::public("refundAddress(address)")]
	// #[precompile::public("refund_address(address)")]
	// #[precompile::view]
	// fn refund_address(
	// 	handle: &mut impl PrecompileHandle,
	// 	user_bfc_address: Address,
	// ) -> EvmResult<BitcoinAddressString> {
	// 	handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
	// 	let user_bfc_address = Runtime::AddressMapping::into_account_id(user_bfc_address.0);

	// 	let refund_address =
	// 		match BtcRegistrationPoolOf::<Runtime>::registration_pool(user_bfc_address) {
	// 			Some(pair) => BitcoinAddressString::from(pair.refund_address.into_inner()),
	// 			None => BitcoinAddressString::from(vec![]),
	// 		};
	// 	Ok(refund_address)
	// }

	// #[precompile::public("register(string,string,bytes)")]
	// fn register(
	// 	handle: &mut impl PrecompileHandle,
	// 	refund_address: BitcoinAddressString,
	// 	vault_address: BitcoinAddressString,
	// 	signature: SignatureBytes,
	// ) -> EvmResult {
	// 	let caller = handle.context().caller;
	// 	let event = log1(
	// 		handle.context().address,
	// 		SELECTOR_LOG_REGISTERED,
	// 		solidity::encode_event_data((
	// 			Address(caller),
	// 			refund_address.clone(),
	// 			vault_address.clone(),
	// 		)),
	// 	);
	// 	handle.record_log_costs(&[&event])?;

	// 	let refund_address =
	// 		Self::convert_string_to_bitcoin_address(refund_address).in_field("refund_address")?;
	// 	let vault_address =
	// 		Self::convert_string_to_bitcoin_address(vault_address).in_field("vault_address")?;
	// 	let signature = EthereumSignature::new(
	// 		Self::convert_bytes_to_signature(signature).in_field("signature")?,
	// 	);

	// 	let call = BtcRegistrationPoolCall::<Runtime>::register {
	// 		refund_address: refund_address.to_vec(),
	// 		vault_address: vault_address.to_vec(),
	// 		signature,
	// 	};
	// 	let origin = Runtime::AddressMapping::into_account_id(caller);
	// 	RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

	// 	event.record(handle)?;

	// 	Ok(())
	// }

	// /// Converts a solidity string typed Bitcoin address to a `BoundedVec`.
	// fn convert_string_to_bitcoin_address(
	// 	string: BitcoinAddressString,
	// ) -> MayRevert<BoundedBitcoinAddress> {
	// 	BoundedVec::try_from(string.as_bytes().to_vec())
	// 		.map_err(|_| RevertReason::custom("invalid bytes").into())
	// }

	// /// Converts a solidity bytes typed signature to a `Signature`.
	// fn convert_bytes_to_signature(bytes: SignatureBytes) -> MayRevert<Signature> {
	// 	Signature::try_from(bytes.as_bytes())
	// 		.map_err(|_| RevertReason::custom("invalid bytes").into())
	// }
}
