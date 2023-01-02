use bifrost_mainnet_runtime::AccountId;

use hex_literal::hex;

/// Returns master account - holds initial funds
pub fn get_master_account() -> AccountId {
	AccountId::from(hex!("13dE5f5929E7E0ca03f176A2403e7Bc32941aBb0"))
}

/// Returns sudo account
pub fn get_sudo_account() -> AccountId {
	AccountId::from(hex!("07712D727bC6c4933317d1C13AAd3beAAb0d1474"))
}

/// Return council member accounts
pub fn get_council_member_accounts() -> Vec<AccountId> {
	vec![
		AccountId::from(hex!("7432cbc34902f0C1300618Ff296EC5dFb5925330")),
		AccountId::from(hex!("064af070EA237431cA1233d821c5B05d8BD5dAa9")),
		AccountId::from(hex!("74345C6984A1736e3D0c47f487c748ae07eCaDC0")),
		AccountId::from(hex!("84c8321a4A2255DF4300cFA59C611c423F7269ba")),
		AccountId::from(hex!("9760A49269491D8de7FBA7Dbd27de06cB0319Fb1")),
	]
}

/// Return tech.comm. member accounts
pub fn get_technical_committee_member_accounts() -> Vec<AccountId> {
	vec![
		AccountId::from(hex!("09B8653A508684AB416917FA9D5B1CC414e610E9")),
		AccountId::from(hex!("BB6610dB53fD31f24d867B76cEF2b58ab4107EE5")),
		AccountId::from(hex!("2FAA78ddf99103D230b5aD594cf9016BDFDd1282")),
		AccountId::from(hex!("A594f0A60b98fF97E1016603962fB083E7D87d77")),
		AccountId::from(hex!("747f1cDBE8cE97cf5b104E9317Bc383437007908")),
	]
}

/// Returns validator accounts (stash address, controller address, relayer address)
pub fn get_validator_accounts() -> Vec<(AccountId, AccountId, AccountId)> {
	vec![
		(
			AccountId::from(hex!("D119f859BFB746Ca586B15feFcd303A5d1ad89DD")),
			AccountId::from(hex!("313365289db06bAF16989732376A7962c24e8968")),
			AccountId::from(hex!("ea3bBe431C0aF2B1AbF59BfcF46Be98Ae06F83b5")),
		),
		(
			AccountId::from(hex!("60fc766090b6A707927d89aC974be95173B9Ef59")),
			AccountId::from(hex!("75c19b28e9d1A74AD4b0EFC8112321102B2337a4")),
			AccountId::from(hex!("bb37F126B266CC877d31047F519912768018237b")),
		),
		(
			AccountId::from(hex!("6D39eCD049F73482Da3e8fc8883eC2aCc94a4Af9")),
			AccountId::from(hex!("E1699192f7e221c0382C100caa193A55b7fA2034")),
			AccountId::from(hex!("3faC4329ba95D1e6197d8c1e2A3DaA2694B9D9f5")),
		),
		(
			AccountId::from(hex!("71eD61aAE03523E90B58BacB976c17CcCa2554E4")),
			AccountId::from(hex!("A76be130D9017994FBB53a67a48e699dDf46AEE4")),
			AccountId::from(hex!("2102334Ef31189424208b83ed32b37ccD3B9EB00")),
		),
	]
}
