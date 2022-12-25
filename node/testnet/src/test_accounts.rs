use bifrost_testnet_runtime::AccountId;

use hex_literal::hex;

/// Returns master account - holds initial funds
pub fn get_master_account() -> AccountId {
	AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac"))
}

/// Returns sudo account
pub fn get_sudo_account() -> AccountId {
	AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac"))
}

/// Return council member accounts
pub fn get_council_member_accounts() -> Vec<AccountId> {
	vec![]
}

/// Returns validator accounts (stash address, controller address, relayer address)
pub fn get_validator_accounts() -> Vec<(AccountId, AccountId, AccountId)> {
	vec![]
}
