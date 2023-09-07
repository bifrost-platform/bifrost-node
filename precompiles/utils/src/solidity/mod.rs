//! Provides utilities for compatibility with Solidity tooling.

pub mod codec;
pub mod modifier;
pub mod revert;

pub use codec::{
	decode_arguments, decode_event_data, decode_return_value, encode_arguments, encode_event_data,
	encode_return_value, encode_with_selector, Codec,
};
