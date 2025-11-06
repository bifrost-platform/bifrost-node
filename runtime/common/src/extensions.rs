use frame_support::pallet_prelude::{TransactionSource, TransactionValidityError};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

use sp_core::H160;
use sp_runtime::{
	traits::{AsSystemOriginSigner, DispatchInfoOf, Dispatchable, TransactionExtension},
	Weight,
};
use sp_std::{
	fmt::{Debug, Formatter},
	marker::PhantomData,
};

/// Transaction extension that blocks extrinsics from specific origins
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo, DecodeWithMemTracking)]
#[scale_info(skip_type_params(T))]
#[codec(encode_bound())]
#[codec(decode_bound())]
pub struct CheckBlockedOrigin<T, Call>(PhantomData<(T, Call)>);

impl<T, Call> Debug for CheckBlockedOrigin<T, Call> {
	fn fmt(&self, f: &mut Formatter) -> sp_std::fmt::Result {
		write!(f, "CheckBlockedOrigin")
	}
}

impl<T, Call> Default for CheckBlockedOrigin<T, Call> {
	fn default() -> Self {
		Self::new()
	}
}

impl<T, Call> CheckBlockedOrigin<T, Call> {
	pub fn new() -> Self {
		Self(PhantomData)
	}
}

impl<T, Call> TransactionExtension<Call> for CheckBlockedOrigin<T, Call>
where
	T: frame_system::Config + Send + Sync,
	T::AccountId: From<H160>,
	T: pallet_bfc_utility::Config,
	Call: Dispatchable + Clone + Eq + TypeInfo + Send + Sync + 'static,
	<Call as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<T::AccountId>,
{
	const IDENTIFIER: &'static str = "CheckBlockedOrigin";
	type Implicit = ();
	type Pre = ();
	type Val = ();

	fn weight(&self, _call: &Call) -> Weight {
		Weight::zero()
	}

	fn validate(
		&self,
		origin: <Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> Result<
		(
			sp_runtime::transaction_validity::ValidTransaction,
			Self::Val,
			<Call as Dispatchable>::RuntimeOrigin,
		),
		TransactionValidityError,
	> {
		if let Some(who) = origin.as_system_origin_signer() {
			if pallet_bfc_utility::Pallet::<T>::is_blocked_account(who) {
				return Err(TransactionValidityError::Invalid(
					sp_runtime::transaction_validity::InvalidTransaction::BadSigner,
				));
			}
		}
		Ok((sp_runtime::transaction_validity::ValidTransaction::default(), (), origin))
	}

	fn prepare(
		self,
		_val: Self::Val,
		_origin: &<Call as Dispatchable>::RuntimeOrigin,
		_call: &Call,
		_info: &DispatchInfoOf<Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(())
	}
}
