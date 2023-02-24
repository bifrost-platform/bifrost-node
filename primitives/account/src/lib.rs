//! The Ethereum Signature implementation.
//! It includes the Verify and IdentifyAccount traits for the AccountId20

#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sha3::{Digest, Keccak256};
use sp_core::{ecdsa, H160, H256};

#[cfg(feature = "std")]
pub use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// The account type to be used in BIFROST. It is a wrapper for 20 fixed bytes. We prefer to use
/// a dedicated type to prevent using arbitrary 20 byte arrays were AccountIds are expected. With
/// the introduction of the `scale-info` crate this benefit extends even to non-Rust tools like
/// Polkadot JS.
#[derive(
	Eq, PartialEq, Copy, Clone, Encode, Decode, TypeInfo, MaxEncodedLen, Default, PartialOrd, Ord,
)]
pub struct AccountId20(pub [u8; 20]);

#[cfg(feature = "std")]
impl_serde::impl_fixed_hash_serde!(AccountId20, 20);

#[cfg(feature = "std")]
impl std::fmt::Display for AccountId20 {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self.0)
	}
}

impl core::fmt::Debug for AccountId20 {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "{:?}", H160(self.0))
	}
}

impl From<[u8; 20]> for AccountId20 {
	fn from(bytes: [u8; 20]) -> Self {
		Self(bytes)
	}
}

impl Into<[u8; 20]> for AccountId20 {
	fn into(self) -> [u8; 20] {
		self.0
	}
}

impl From<H160> for AccountId20 {
	fn from(h160: H160) -> Self {
		Self(h160.0)
	}
}

impl Into<H160> for AccountId20 {
	fn into(self) -> H160 {
		H160(self.0)
	}
}

#[cfg(feature = "std")]
impl std::str::FromStr for AccountId20 {
	type Err = &'static str;
	fn from_str(input: &str) -> Result<Self, Self::Err> {
		H160::from_str(input).map(Into::into).map_err(|_| "invalid hex address.")
	}
}

#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[derive(Eq, PartialEq, Clone, Encode, Decode, sp_core::RuntimeDebug, TypeInfo)]
pub struct EthereumSignature(ecdsa::Signature);

impl From<ecdsa::Signature> for EthereumSignature {
	fn from(x: ecdsa::Signature) -> Self {
		EthereumSignature(x)
	}
}

impl sp_runtime::traits::Verify for EthereumSignature {
	type Signer = EthereumSigner;
	fn verify<L: sp_runtime::traits::Lazy<[u8]>>(&self, mut msg: L, signer: &AccountId20) -> bool {
		let mut m = [0u8; 32];
		m.copy_from_slice(Keccak256::digest(msg.get()).as_slice());
		match sp_io::crypto::secp256k1_ecdsa_recover(self.0.as_ref(), &m) {
			Ok(pubkey) =>
				AccountId20(H160::from(H256::from_slice(Keccak256::digest(&pubkey).as_slice())).0) ==
					*signer,
			Err(sp_io::EcdsaVerifyError::BadRS) => {
				log::error!(target: "evm", "Error recovering: Incorrect value of R or S");
				false
			},
			Err(sp_io::EcdsaVerifyError::BadV) => {
				log::error!(target: "evm", "Error recovering: Incorrect value of V");
				false
			},
			Err(sp_io::EcdsaVerifyError::BadSignature) => {
				log::error!(target: "evm", "Error recovering: Invalid signature");
				false
			},
		}
	}
}

/// Public key for an Ethereum / BIFROST compatible account
#[derive(
	Eq, PartialEq, Ord, PartialOrd, Clone, Encode, Decode, sp_core::RuntimeDebug, TypeInfo,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct EthereumSigner([u8; 20]);

impl sp_runtime::traits::IdentifyAccount for EthereumSigner {
	type AccountId = AccountId20;
	fn into_account(self) -> AccountId20 {
		AccountId20(self.0)
	}
}

impl From<[u8; 20]> for EthereumSigner {
	fn from(x: [u8; 20]) -> Self {
		EthereumSigner(x)
	}
}

impl From<ecdsa::Public> for EthereumSigner {
	fn from(x: ecdsa::Public) -> Self {
		let decompressed = libsecp256k1::PublicKey::parse_slice(
			&x.0,
			Some(libsecp256k1::PublicKeyFormat::Compressed),
		)
		.expect("Wrong compressed public key provided")
		.serialize();
		let mut m = [0u8; 64];
		m.copy_from_slice(&decompressed[1..65]);
		let account = H160::from(H256::from_slice(Keccak256::digest(&m).as_slice()));
		EthereumSigner(account.into())
	}
}

impl From<libsecp256k1::PublicKey> for EthereumSigner {
	fn from(x: libsecp256k1::PublicKey) -> Self {
		let mut m = [0u8; 64];
		m.copy_from_slice(&x.serialize()[1..65]);
		let account = H160::from(H256::from_slice(Keccak256::digest(&m).as_slice()));
		EthereumSigner(account.into())
	}
}

#[cfg(feature = "std")]
impl std::fmt::Display for EthereumSigner {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(fmt, "ethereum signature: {:?}", H160::from_slice(&self.0))
	}
}
