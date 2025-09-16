use sp_consensus::block_validation::BlockAnnounceValidator;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::future::Future;
use std::pin::Pin;

/// Custom block announce validator that skips block #336000
pub struct BlockSkipValidator;

impl BlockSkipValidator {
	pub fn new() -> Self {
		Self
	}
}

impl<Block> BlockAnnounceValidator<Block> for BlockSkipValidator
where
	Block: BlockT,
{
	fn validate(
		&mut self,
		header: &Block::Header,
		_data: &[u8],
	) -> Pin<
		Box<
			dyn Future<
					Output = Result<
						sp_consensus::block_validation::Validation,
						Box<dyn std::error::Error + Send>,
					>,
				> + Send,
		>,
	> {
		let block_number = *header.number();

		Box::pin(async move {
			// Skip block 336000 by rejecting it
			if block_number == 336000u32.into() {
				log::info!("Skipping block 336000 during sync");
				return Ok(sp_consensus::block_validation::Validation::Failure {
					disconnect: false,
				});
			}

			// Accept all other blocks
			Ok(sp_consensus::block_validation::Validation::Success { is_new_best: false })
		})
	}
}
