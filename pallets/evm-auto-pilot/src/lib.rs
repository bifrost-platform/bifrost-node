#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub use pallet::pallet::*;

pub mod weights;
use weights::WeightInfo;
