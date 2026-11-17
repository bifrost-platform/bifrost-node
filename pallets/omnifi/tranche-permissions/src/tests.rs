use crate::{mock::*, Error, ProductAdmins, TrancheInvestors};
use frame_support::{assert_noop, assert_ok};
use pallet_tranche_system::{ChainAddress, VaultId};
use sp_core::{H160, H256};

const PID: u64 = 1;
const ADMIN: u64 = 7;

fn vault(chain_id: u64) -> VaultId {
	VaultId { chain_id, vault_address: H160::from_low_u64_be(1).into() }
}

#[test]
fn an_investor_must_be_a_canonical_address_on_a_registered_chain() {
	new_test_ext().execute_with(|| {
		ProductAdmins::<Test>::insert(PID, ADMIN);
		let grant = |chain_id, investor| {
			TranchePermissions::grant_tranche_investor(
				RuntimeOrigin::signed(ADMIN),
				PID,
				vault(chain_id),
				investor,
			)
		};
		let canonical: ChainAddress = H160::repeat_byte(0xab).into();
		// the same low 20 bytes with a non-zero padding byte
		let mut alias = canonical;
		alias.0[0] = 1;

		assert_noop!(grant(UNREGISTERED_CHAIN, canonical), Error::<Test>::ChainNotRegistered);
		assert_noop!(grant(EVM_CHAIN, alias), Error::<Test>::InvalidInvestorAddress);
		assert_ok!(grant(EVM_CHAIN, canonical));
		// a 32-byte chain takes any address
		assert_ok!(grant(2, H256::repeat_byte(0xcd)));

		// an entry stored before the check existed can still be revoked
		TrancheInvestors::<Test>::insert((PID, vault(UNREGISTERED_CHAIN), alias), ());
		assert_ok!(TranchePermissions::revoke_tranche_investor(
			RuntimeOrigin::signed(ADMIN),
			PID,
			vault(UNREGISTERED_CHAIN),
			alias,
		));
	});
}
