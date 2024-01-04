#![cfg_attr(not(feature = "std"), no_std)]

use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use bp_staking::TierType;
use fp_evm::PrecompileHandle;
use sp_core::{H160, H256};
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData, vec, vec::Vec};

mod types;
use types::{
	EvmValidatorOffenceOf, EvmValidatorOffencesOf, OffencesOf, ValidatorOffence, ValidatorOffences,
};

/// A precompile to wrap the functionality from pallet_bfc_offences
pub struct BfcOffencesPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> BfcOffencesPrecompile<Runtime>
where
	Runtime: pallet_bfc_offences::Config + pallet_evm::Config + frame_system::Config,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
{
	// Storage getters

	#[precompile::public("maximumOffenceCount(uint256)")]
	#[precompile::public("maximum_offence_count(uint256)")]
	#[precompile::view]
	fn maximum_offence_count(handle: &mut impl PrecompileHandle, tier: u32) -> EvmResult<Vec<u32>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let tier = match tier {
			2 => TierType::Full,
			1 => TierType::Basic,
			0 => TierType::All,
			_ => return Err(RevertReason::read_out_of_bounds("tier").into()),
		};

		let mut maximum_offence_count = vec![];
		match tier {
			TierType::Full => {
				maximum_offence_count.push(OffencesOf::<Runtime>::full_maximum_offence_count());
			},
			TierType::Basic => {
				maximum_offence_count.push(OffencesOf::<Runtime>::basic_maximum_offence_count());
			},
			TierType::All => {
				maximum_offence_count.push(OffencesOf::<Runtime>::full_maximum_offence_count());
				maximum_offence_count.push(OffencesOf::<Runtime>::basic_maximum_offence_count());
			},
		}

		Ok(maximum_offence_count)
	}

	#[precompile::public("validatorOffence(address)")]
	#[precompile::public("validator_offence(address)")]
	#[precompile::view]
	fn validator_offence(
		handle: &mut impl PrecompileHandle,
		validator: Address,
	) -> EvmResult<EvmValidatorOffenceOf> {
		let validator = Runtime::AddressMapping::into_account_id(validator.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		if let Some(offence) = OffencesOf::<Runtime>::validator_offences(&validator) {
			let mut new = ValidatorOffence::<Runtime>::default();
			new.set_offence(validator, offence);
			Ok(new.into())
		} else {
			Ok(ValidatorOffence::<Runtime>::default().into())
		}
	}

	#[precompile::public("validatorOffences(address[])")]
	#[precompile::public("validator_offences(address[])")]
	#[precompile::view]
	fn validator_offences(
		handle: &mut impl PrecompileHandle,
		validators: Vec<Address>,
	) -> EvmResult<EvmValidatorOffencesOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let unique_validators: BTreeSet<Runtime::AccountId> = validators
			.iter()
			.map(|validator| Runtime::AddressMapping::into_account_id(validator.0))
			.collect();
		if unique_validators.len() != validators.len() {
			return Err(RevertReason::custom("Duplicate validator address received").into());
		}

		let mut validator_offences = ValidatorOffences::<Runtime>::default();
		unique_validators.clone().into_iter().for_each(|v| {
			if let Some(offence) = OffencesOf::<Runtime>::validator_offences(&v) {
				let mut new = ValidatorOffence::<Runtime>::default();
				new.set_offence(v, offence);
				validator_offences.insert_offence(new);
			} else {
				validator_offences.insert_empty(v);
			}
		});

		Ok(validator_offences.into())
	}
}
