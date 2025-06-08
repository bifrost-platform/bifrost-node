#![cfg_attr(not(feature = "std"), no_std)]

pub mod currency {
	use bp_core::Balance;

	pub const SUPPLY_FACTOR: Balance = 1;
	pub const UNITS: Balance = 1_000_000_000_000_000_000;

	pub const BFC: Balance = UNITS; // 1_000_000_000_000_000_000
	pub const MILLIBFC: Balance = BFC / 1_000; // 1_000_000_000_000_000
	pub const MICROBFC: Balance = BFC / 1_000_000; // 1_000_000_000_000

	pub const WEI: Balance = 1;
	pub const GWEI: Balance = WEI * 1_000_000_000; // 1_000_000_000

	pub const TRANSACTION_BYTE_FEE: Balance = 10 * SUPPLY_FACTOR * MICROBFC;
	pub const STORAGE_BYTE_FEE: Balance = 1 * SUPPLY_FACTOR * MILLIBFC;
}

pub mod time {
	use bp_core::BlockNumber;

	/// This determines the average expected block time that we are targeting.
	/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
	/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
	/// up by `pallet_aura` to implement `fn slot_duration()`.
	pub const MILLISECS_PER_BLOCK: u64 = 3000;

	/// Currently it is not possible to change the slot duration after the chain has started.
	/// Attempting to do so will brick block production.
	pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

	/// Time is measured by number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = 60 * MINUTES;
	pub const DAYS: BlockNumber = 24 * HOURS;
}
