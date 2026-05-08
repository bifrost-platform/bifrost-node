use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Current block number as u32.
	pub fn current_block() -> u32 {
		frame_system::Pallet::<T>::block_number().try_into().unwrap_or(u32::MAX)
	}
}
