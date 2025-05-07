use crate::Script;
use miniscript::bitcoin::script::Instruction;
use miniscript::bitcoin::{opcodes, ScriptBuf, TxIn, Weight};
use sp_std::vec::Vec;

/// Parse the witness script for extract m and n for m-of-n multisig.
/// return None if the script is not a valid multisig script.
fn parse_multisig_script(script: &Script) -> Option<(usize, usize)> {
	let instructions = script.instructions().collect::<Vec<_>>();

	if instructions.len() < 3 {
		return None; // Not enough instructions for multisig
	}

	// First instruction should be the number of required signatures (m)
	let m = match instructions[0] {
		Ok(Instruction::Op(op)) if op.to_u8() >= 0x51 && op.to_u8() <= 0x60 => {
			(op.to_u8() - 0x51 + 1) as usize
		},
		_ => return None, // Not a valid multisig script
	};

	// Last instruction should be OP_CHECKMULTISIG opcode
	if let Ok(Instruction::Op(opcodes::all::OP_CHECKMULTISIG)) = instructions.last().unwrap() {
		// Second-to-last instruction should be the number of public keys (n)
		let n = match instructions[instructions.len() - 2] {
			Ok(Instruction::Op(op)) if op.to_u8() >= 0x51 && op.to_u8() <= 0x60 => {
				(op.to_u8() - 0x51 + 1) as usize
			},
			_ => return None, // Not a valid multisig script
		};

		Some((m, n))
	} else {
		None
	}
}

/// Estimate the finalized input vsize from unsigned.
pub fn estimate_finalized_input_size(
	witness_script: &ScriptBuf,
	txin: Option<&TxIn>,
) -> Option<u64> {
	let (m, _) = if let Some((m, n)) = parse_multisig_script(witness_script) {
		(m, n)
	} else {
		return None;
	};

	let script_len = witness_script.len() + 1;

	// empty(1byte) + signatures(73 * m) + script_len
	let estimated_witness_size = 1 + 73 * m + script_len;

	let non_witness_data_size = if let Some(txin) = txin {
		Weight::from_non_witness_data_size(txin.base_size() as u64)
	} else {
		Weight::from_non_witness_data_size(41)
	};

	let estimated_final_vsize = (Weight::from_witness_data_size(estimated_witness_size as u64)
		+ non_witness_data_size)
		.to_vbytes_ceil();

	Some(estimated_final_vsize)
}
