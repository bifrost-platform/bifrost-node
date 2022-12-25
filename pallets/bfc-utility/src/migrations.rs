use super::*;
use frame_support::pallet_prelude::Weight;

pub mod v2 {
	use super::*;
	use frame_support::traits::Get;

	#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	pub struct OldProposal<AccountId> {
		pub who: AccountId,
		pub proposal_hex: String,
		pub proposal_index: PropIndex,
	}

	pub fn migrate<T: Config>() -> Weight {
		let mut new_proposals: Vec<Proposal> = vec![];
		AcceptedProposals::<T>::translate(|v: Option<Vec<OldProposal<T::AccountId>>>| {
			if let Some(proposals) = v.clone() {
				for proposal in proposals {
					new_proposals.push(Proposal {
						proposal_hex: proposal.proposal_hex,
						proposal_index: proposal.proposal_index,
					});
				}
				Some(new_proposals)
			} else {
				// For Safety. Should not happen
				None
			}
		})
		.expect("BFC_UTILITY: Error while migrating");

		StorageVersion::<T>::put(Releases::V2_0_0);
		T::BlockWeights::get().max_block
	}
}
