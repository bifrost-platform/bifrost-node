//! Solana program derived address (PDA) precompile — `derive_pda(bytes32, bytes[])`.
//!
//! A pure view: the hub predicts a Solana spoke's PDAs (vault, share mint, multichain adapter)
//! before the spoke deploys them, the same way it predicts EVM spokes with CREATE2
//! (omnifi-tranche `docs/multichain-v2/node-request-pda-precompile.md`, "N6").
//!
//! The result is byte-for-byte Agave's `Pubkey::find_program_address`:
//!
//! ```text
//! for bump in 255, 254, …, 1:
//!     h = sha256(seeds[0] ‖ … ‖ seeds[n−1] ‖ [bump] ‖ program_id ‖ "ProgramDerivedAddress")
//!     if h is not a valid ed25519 point: return (h, bump)
//! revert
//! ```
//!
//! - seeds are concatenated with no separator or length prefix; at most 15 seeds (Solana's 16
//!   including the bump seed), each at most 32 bytes. Bump 0 is never tried (Agave loops
//!   `0..u8::MAX` from 255).
//! - "valid ed25519 point" is `curve25519_dalek::edwards::CompressedEdwardsY::decompress()`
//!   succeeding, with the same `curve25519-dalek` 4.x Agave uses.
//!
//! No state, no caller or block dependence: the same input always gives the same output.

#![cfg_attr(not(feature = "std"), no_std)]

use curve25519_dalek::edwards::CompressedEdwardsY;
use precompile_utils::{prelude::*, solidity::codec::UnboundedBytes};
use sp_core::H256;
use sp_std::vec::Vec;

/// Most seeds a caller may pass (Solana's `MAX_SEEDS` = 16 counts the bump seed).
pub const MAX_SEEDS: usize = 15;
/// Longest seed (Solana's `MAX_SEED_LEN`).
pub const MAX_SEED_LEN: usize = 32;
/// The marker Solana appends to every PDA preimage.
pub const PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";

/// Charged once per call (decoding, preimage assembly).
pub const BASE_GAS: u64 = 3_000;
/// Charged per bump tried, before trying it: one sha256 (host function) and one ed25519
/// decompression (in the runtime). Most PDAs are found within 1–8 tries; the worst case (255
/// tries) stays far below the block gas limit.
pub const GAS_PER_ATTEMPT: u64 = 4_000;
/// Charged per 32-byte word hashed in each try (the Ethereum `sha256` precompile's word price).
pub const GAS_PER_HASHED_WORD: u64 = 12;

/// Is `bytes` the compressed form of a point on the ed25519 curve? Exactly Agave's
/// `bytes_are_curve_point`.
pub fn is_on_curve(bytes: &[u8; 32]) -> bool {
	CompressedEdwardsY(*bytes).decompress().is_some()
}

/// The preimage of one try: `seeds ‖ [bump] ‖ program_id ‖ marker`, with the bump byte at
/// `seeds_len`.
fn preimage(seeds: &[&[u8]], program_id: &[u8; 32]) -> (Vec<u8>, usize) {
	let seeds_len: usize = seeds.iter().map(|s| s.len()).sum();
	let mut buf = Vec::with_capacity(seeds_len + 1 + 32 + PDA_MARKER.len());
	for s in seeds {
		buf.extend_from_slice(s);
	}
	buf.push(0);
	buf.extend_from_slice(program_id);
	buf.extend_from_slice(PDA_MARKER);
	(buf, seeds_len)
}

/// Why a derivation is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdaError {
	TooManySeeds,
	SeedTooLong,
	NoViableBump,
}

impl PdaError {
	pub fn message(&self) -> &'static str {
		match self {
			PdaError::TooManySeeds => "too many seeds (max 15)",
			PdaError::SeedTooLong => "seed longer than 32 bytes",
			PdaError::NoViableBump => "no viable bump seed",
		}
	}
}

fn check_seeds(seeds: &[&[u8]]) -> Result<(), PdaError> {
	if seeds.len() > MAX_SEEDS {
		return Err(PdaError::TooManySeeds);
	}
	if seeds.iter().any(|s| s.len() > MAX_SEED_LEN) {
		return Err(PdaError::SeedTooLong);
	}
	Ok(())
}

/// Agave `Pubkey::find_program_address`. `before_attempt` runs before each bump is tried (the
/// precompile charges gas there) and can stop the search.
pub fn find_program_address_with<E>(
	seeds: &[&[u8]],
	program_id: &[u8; 32],
	mut before_attempt: impl FnMut() -> Result<(), E>,
) -> Result<Result<([u8; 32], u8), PdaError>, E> {
	if let Err(e) = check_seeds(seeds) {
		return Ok(Err(e));
	}
	let (mut buf, bump_at) = preimage(seeds, program_id);
	for bump in (1..=u8::MAX).rev() {
		before_attempt()?;
		buf[bump_at] = bump;
		let h = sp_io::hashing::sha2_256(&buf);
		if !is_on_curve(&h) {
			return Ok(Ok((h, bump)));
		}
	}
	Ok(Err(PdaError::NoViableBump))
}

/// Agave `Pubkey::find_program_address`, without metering.
pub fn find_program_address(
	seeds: &[&[u8]],
	program_id: &[u8; 32],
) -> Result<([u8; 32], u8), PdaError> {
	match find_program_address_with::<()>(seeds, program_id, || Ok(())) {
		Ok(r) => r,
		Err(()) => unreachable!("the unmetered search never stops early"),
	}
}

/// Gas of one try for a preimage of `seeds_len + 54` bytes.
fn attempt_gas(seeds_len: usize) -> u64 {
	let words = (seeds_len + 1 + 32 + PDA_MARKER.len()).div_ceil(32) as u64;
	GAS_PER_ATTEMPT.saturating_add(GAS_PER_HASHED_WORD.saturating_mul(words))
}

/// The precompile. Stateless, so not generic over the runtime.
pub struct SvmPdaPrecompile;

#[precompile_utils::precompile]
impl SvmPdaPrecompile {
	/// Solana PDA of `seeds` under `program_id` (an ed25519 public key, raw 32 bytes — not a
	/// padded EVM address). Returns the address and the bump found (255 … 1).
	#[precompile::public("derive_pda(bytes32,bytes[])")]
	#[precompile::view]
	fn derive_pda(
		handle: &mut impl PrecompileHandle,
		program_id: H256,
		seeds: Vec<UnboundedBytes>,
	) -> EvmResult<(H256, u8)> {
		handle.record_cost(BASE_GAS)?;
		let seeds: Vec<Vec<u8>> = seeds.into_iter().map(Into::into).collect();
		let refs: Vec<&[u8]> = seeds.iter().map(|s| s.as_slice()).collect();
		let per_try = attempt_gas(refs.iter().map(|s| s.len()).sum());
		match find_program_address_with(&refs, &program_id.0, || handle.record_cost(per_try))? {
			Ok((addr, bump)) => Ok((H256(addr), bump)),
			Err(e) => Err(revert(e.message())),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use fp_evm::Precompile;
	use precompile_utils::{solidity, testing::MockHandle};
	use sp_core::H160;

	fn selector(sig: &str) -> u32 {
		let h = sp_io::hashing::keccak_256(sig.as_bytes());
		u32::from_be_bytes([h[0], h[1], h[2], h[3]])
	}

	/// devnet `omnifi_spoke` = GMJ4MMCHFqTre4Uo8xvnx8DozNnLeJXYCwcTCf98JYfj
	const PROGRAM: [u8; 32] =
		hex_lit("e4123515af730d428007e52c3bf42ce29e5258c062e922acd34bfb645b5b6e3a");

	const fn hex_lit(s: &str) -> [u8; 32] {
		let b = s.as_bytes();
		let mut out = [0u8; 32];
		let mut i = 0;
		while i < 32 {
			out[i] = (nib(b[2 * i]) << 4) | nib(b[2 * i + 1]);
			i += 1;
		}
		out
	}

	const fn nib(c: u8) -> u8 {
		match c {
			b'0'..=b'9' => c - b'0',
			b'a'..=b'f' => c - b'a' + 10,
			_ => panic!("bad hex"),
		}
	}

	/// The nine vectors of the spec (§6), from solana-cli 4.3.0 `find-program-derived-address`.
	#[test]
	fn matches_the_spec_vectors() {
		let p3 = 0x0000_0004_0000_0003u64.to_be_bytes();
		let p1 = 0x0000_0002_0000_0001u64.to_be_bytes();
		let cases: [(&[&[u8]], u8, &str); 9] = [
			(
				&[b"tranche", &p3, &[0]],
				249,
				"fa11543a319a5d6ddae81648b437641ed4eee9b13efccdb07d860e3cfcd60c91",
			),
			(
				&[b"tranche", &p3, &[1]],
				255,
				"10ce9562aac5e14da569df1ea0ce5ec366966764b109ccfe933ed4b052d79fbb",
			),
			(
				&[b"tranche", &p3, &[2]],
				253,
				"bfc873df35816d1c1315a6d9b505ed34cbdb2c16d79aaecef4cc1e105903a321",
			),
			(
				&[b"share", &p3, &[0]],
				252,
				"dac84d1af4ba45505711929e6a935da811b43d99b7a0623245efc8271ee1cffc",
			),
			(
				&[b"share", &p3, &[1]],
				253,
				"c4f673045519d22c1b1a0049e91d57ef9752156eb0bb6fbf40c9202f08983705",
			),
			(
				&[b"mc", &p3],
				255,
				"719bbf70514bda6ad97570104c9a3dad04b4e8dbc8da636dd94a42e0bf1da625",
			),
			(
				&[b"tranche", &p1, &[0]],
				250,
				"37fa58ba88e690c2e1d4479e5fa65ac8767b57fe685e5f28dd1fb1e8a0be75dd",
			),
			(
				&[b"share", &p1, &[0]],
				251,
				"a4e56a1b4f6caa0d22c28a7af1abd86cac10a25d6e1853bad8a2d6de690dcda5",
			),
			(
				&[b"mc", &p1],
				254,
				"fa141dcacb6770dc648d6f989904f668682e312487cf56c4d873efa9d01300c9",
			),
		];
		for (i, (seeds, bump, addr)) in cases.iter().enumerate() {
			assert_eq!(
				find_program_address(seeds, &PROGRAM),
				Ok((hex_lit(addr), *bump)),
				"vector {}",
				i + 1
			);
		}
		// vector 1 skipped six on-curve candidates (255 … 250)
		let (mut buf, at) = preimage(cases[0].0, &PROGRAM);
		for b in 250..=255u8 {
			buf[at] = b;
			assert!(is_on_curve(&sp_io::hashing::sha2_256(&buf)), "bump {b}");
		}
	}

	#[test]
	fn seed_limits() {
		let long = [7u8; 33];
		let max = [7u8; 32];
		assert_eq!(find_program_address(&[&long], &PROGRAM), Err(PdaError::SeedTooLong));
		let sixteen: Vec<&[u8]> = (0..16).map(|_| &max[..]).collect();
		assert_eq!(find_program_address(&sixteen, &PROGRAM), Err(PdaError::TooManySeeds));
		// the limits themselves are fine, and so are no seeds and an empty seed
		let fifteen: Vec<&[u8]> = (0..15).map(|_| &max[..]).collect();
		assert!(find_program_address(&fifteen, &PROGRAM).is_ok());
		assert!(find_program_address(&[], &PROGRAM).is_ok());
		assert_eq!(
			find_program_address(&[b""], &PROGRAM),
			find_program_address(&[], &PROGRAM),
			"empty seeds add no bytes"
		);
	}

	fn call(input: Vec<u8>, gas_limit: u64) -> (fp_evm::PrecompileResult, u64) {
		let mut h = MockHandle::new(
			H160::from_low_u64_be(0x0900),
			fp_evm::Context {
				address: H160::from_low_u64_be(0x0900),
				caller: H160::repeat_byte(0x11),
				apparent_value: Default::default(),
			},
		);
		h.input = input;
		h.gas_limit = gas_limit;
		h.is_static = true;
		let r = <SvmPdaPrecompile as Precompile>::execute(&mut h);
		(r, h.gas_used)
	}

	/// The calldata and return data of the spec's worked example (§6, vector 1).
	#[test]
	fn abi_matches_the_spec_example() {
		let input = hex_bytes(concat!(
			"9d646a92",
			"e4123515af730d428007e52c3bf42ce29e5258c062e922acd34bfb645b5b6e3a",
			"0000000000000000000000000000000000000000000000000000000000000040",
			"0000000000000000000000000000000000000000000000000000000000000003",
			"0000000000000000000000000000000000000000000000000000000000000060",
			"00000000000000000000000000000000000000000000000000000000000000a0",
			"00000000000000000000000000000000000000000000000000000000000000e0",
			"0000000000000000000000000000000000000000000000000000000000000007",
			"7472616e63686500000000000000000000000000000000000000000000000000",
			"0000000000000000000000000000000000000000000000000000000000000008",
			"0000000400000003000000000000000000000000000000000000000000000000",
			"0000000000000000000000000000000000000000000000000000000000000001",
			"0000000000000000000000000000000000000000000000000000000000000000",
		));
		assert_eq!(&input[..4], &selector("derive_pda(bytes32,bytes[])").to_be_bytes());
		let (out, gas) = call(input, u64::MAX);
		let out = out.expect("succeeds");
		assert_eq!(
			out.output,
			hex_bytes(concat!(
				"fa11543a319a5d6ddae81648b437641ed4eee9b13efccdb07d860e3cfcd60c91",
				"00000000000000000000000000000000000000000000000000000000000000f9",
			))
		);
		// bump 249 = seven tries over a 70-byte preimage (3 words)
		assert_eq!(gas, BASE_GAS + 7 * (GAS_PER_ATTEMPT + 3 * GAS_PER_HASHED_WORD));
	}

	#[test]
	fn reverts_and_runs_out_of_gas_cleanly() {
		let enc = |seeds: Vec<Vec<u8>>| {
			solidity::encode_with_selector(
				selector("derive_pda(bytes32,bytes[])"),
				(H256(PROGRAM), seeds.into_iter().map(UnboundedBytes::from).collect::<Vec<_>>()),
			)
		};
		let (r, _) = call(enc(vec![vec![0u8; 33]]), u64::MAX);
		assert!(r.is_err());
		let (r, _) = call(enc((0..16).map(|_| vec![1u8]).collect()), u64::MAX);
		assert!(r.is_err());
		// enough gas for the base cost and a few tries, not for vector 1's seven
		let seeds =
			vec![b"tranche".to_vec(), 0x0000_0004_0000_0003u64.to_be_bytes().to_vec(), vec![0]];
		let (r, _) = call(enc(seeds.clone()), BASE_GAS + 3 * GAS_PER_ATTEMPT);
		assert!(
			matches!(
				r,
				Err(fp_evm::PrecompileFailure::Error { exit_status: fp_evm::ExitError::OutOfGas })
			),
			"{r:?}"
		);
		let (r, _) = call(enc(seeds), u64::MAX);
		assert!(r.is_ok());
	}

	/// Cross-check against an independent on-curve test: decompress by hand (the spec's formula,
	/// `(y² − 1)/(d·y² + 1)` a square mod p) for many random preimages.
	#[test]
	fn on_curve_matches_the_formula() {
		let mut x = 0x9e37_79b9_7f4a_7c15u64;
		let mut next = || {
			x ^= x << 13;
			x ^= x >> 7;
			x ^= x << 17;
			x
		};
		let mut on = 0;
		for _ in 0..2_000 {
			let mut b = [0u8; 32];
			for c in b.chunks_mut(8) {
				c.copy_from_slice(&next().to_le_bytes());
			}
			let expected = formula::on_curve(&b);
			assert_eq!(is_on_curve(&b), expected, "{b:?}");
			on += usize::from(expected);
		}
		// about half of all 32-byte strings decode to a point
		assert!((800..1200).contains(&on), "{on}");
	}

	fn hex_bytes(s: &str) -> Vec<u8> {
		(0..s.len())
			.step_by(2)
			.map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
			.collect()
	}

	/// The spec's on-curve formula over GF(2²⁵⁵ − 19), with big integers built from u64 limbs —
	/// slow but independent of curve25519-dalek.
	mod formula {
		use sp_core::U512;

		fn p() -> U512 {
			(U512::one() << 255) - U512::from(19)
		}

		fn mulm(a: U512, b: U512) -> U512 {
			// a, b < p < 2²⁵⁵ so a·b < 2⁵¹⁰ fits U512
			(a * b) % p()
		}

		fn powm(mut b: U512, mut e: U512) -> U512 {
			let mut r = U512::one();
			while !e.is_zero() {
				if e.bit(0) {
					r = mulm(r, b);
				}
				b = mulm(b, b);
				e >>= 1;
			}
			r
		}

		fn inv(a: U512) -> U512 {
			powm(a, p() - U512::from(2))
		}

		/// dalek's `decompress`: y = low 255 bits (non-canonical y ≥ p is reduced), point exists
		/// ⇔ u/v is a square (0 included), where u = y² − 1, v = d·y² + 1. The sign bit only picks
		/// ±x.
		pub fn on_curve(bytes: &[u8; 32]) -> bool {
			let p = p();
			let mut le = *bytes;
			let sign = le[31] >> 7;
			le[31] &= 0x7f;
			let y = U512::from_little_endian(&le) % p;
			let d = mulm(p - U512::from(121_665), inv(U512::from(121_666)));
			let yy = mulm(y, y);
			let u = (yy + p - U512::one()) % p;
			let v = (mulm(d, yy) + U512::one()) % p;
			let x2 = mulm(u, inv(v));
			if x2.is_zero() {
				// x = 0 (y = ±1): a point; dalek 4 does not reject the "negative zero" sign bit
				let _ = sign;
				return true;
			}
			// Euler: x² is a square ⇔ x2^((p−1)/2) = 1
			powm(x2, (p - U512::one()) >> 1) == U512::one()
		}
	}
}
