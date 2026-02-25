use crate::{AssetId, AssetOracleId, ChainId, OracleKey};
use sp_core::H160;

/// Cross-pallet interface for the Oracle Registry.
///
/// Implement this trait on the oracle registry pallet and use it as a bound in
/// other pallets' `Config` to give them typed, read-only access to the oracle
/// registry without creating a hard dependency on the pallet itself.
pub trait OracleRegistryManager {
	/// Returns the oracle ID registered for the given key, or `None` if not
	/// registered.
	fn get_oracle(key: OracleKey) -> Option<AssetOracleId>;

	/// Returns the oracle ID registered for the given EVM asset contract
	/// address, or `None` if not registered.
	fn get_asset_oracle(asset: &AssetId) -> Option<AssetOracleId> {
		Self::get_oracle(OracleKey::Asset(*asset))
	}

	/// Returns the oracle ID registered for the native currency of the given
	/// chain, or `None` if not registered.
	fn get_native_currency_oracle(chain_id: ChainId) -> Option<AssetOracleId> {
		Self::get_oracle(OracleKey::NativeCurrency(chain_id))
	}

	/// Returns the oracle manager contract address, or `None` if not set.
	///
	/// Other pallets (e.g., precompiles) can use this to verify whether a
	/// calling EVM contract is authorised to manage the oracle registry.
	fn get_oracle_manager_contract() -> Option<H160>;
}
