#[macro_export]
macro_rules! impl_self_contained_call {
	{} => {
		// Some kind of ethereum transaction wrapper over FRAME
		// Dispatch every ethereum call by the self contained logic
		// Otherwise dispatch the general FRAME logic
		impl fp_self_contained::SelfContainedCall for Call {
			type SignedInfo = H160;

			fn is_self_contained(&self) -> bool {
				match self {
					Call::Ethereum(call) => call.is_self_contained(),
					_ => false,
				}
			}

			fn check_self_contained(&self) -> Option<Result<Self::SignedInfo, TransactionValidityError>> {
				match self {
					Call::Ethereum(call) => call.check_self_contained(),
					_ => None,
				}
			}

			fn validate_self_contained(
				&self,
				info: &Self::SignedInfo,
				dispatch_info: &DispatchInfoOf<Call>,
				len: usize,
			) -> Option<TransactionValidity> {
				match self {
					Call::Ethereum(call) => call.validate_self_contained(info, dispatch_info, len),
					_ => None,
				}
			}

			fn pre_dispatch_self_contained(
				&self,
				info: &Self::SignedInfo,
				dispatch_info: &DispatchInfoOf<Call>,
				len: usize,
			) -> Option<Result<(), TransactionValidityError>> {
				match self {
					Call::Ethereum(call) => call.pre_dispatch_self_contained(info, dispatch_info, len),
					_ => None,
				}
			}

			fn apply_self_contained(
				self,
				info: Self::SignedInfo,
			) -> Option<sp_runtime::DispatchResultWithInfo<PostDispatchInfoOf<Self>>> {
				match self {
					call @ Call::Ethereum(pallet_ethereum::Call::transact { .. }) => Some(
						call.dispatch(Origin::from(pallet_ethereum::RawOrigin::EthereumTransaction(info))),
					),
					_ => None,
				}
			}
		}
	};
}
