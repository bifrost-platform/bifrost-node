/// Generates the `BifrostPrecompiles<R>` newtype wrapper around a caller-defined
/// `$inner<R>` type (typically `BifrostPrecompilesInner<R>`), together with:
///
/// - `new()` / `Default` impls
/// - A concrete `used_addresses()` impl for `$runtime`
/// - ABI-encoded revert helper (`Error("blocked account")`)
/// - `pallet_evm::PrecompileSet` impl that intercepts calls to blocked accounts
///
/// Each runtime keeps its own `BifrostPrecompilesAt<R>` precompile list and
/// `BifrostPrecompilesInner<R>` type alias, then calls this macro once:
///
/// ```ignore
/// bifrost_common_runtime::impl_bifrost_precompiles!(crate::Runtime, BifrostPrecompilesInner);
/// ```
#[macro_export]
macro_rules! impl_bifrost_precompiles {
	($runtime:ty, $inner:ident) => {
		pub struct BifrostPrecompiles<R>($inner<R>);

		impl<R> BifrostPrecompiles<R>
		where
			R: pallet_evm::Config,
			$inner<R>: Default,
		{
			pub fn new() -> Self {
				Self(Default::default())
			}
		}

		impl BifrostPrecompiles<$runtime> {
			pub fn used_addresses() -> impl Iterator<Item = pallet_evm::AccountIdOf<$runtime>> {
				$inner::<$runtime>::used_addresses()
			}
		}

		impl<R> Default for BifrostPrecompiles<R>
		where
			R: pallet_evm::Config,
			$inner<R>: Default,
		{
			fn default() -> Self {
				Self::new()
			}
		}

		fn blocked_account_revert_data() -> sp_runtime::Vec<u8> {
			const MSG: &[u8] = b"blocked account"; // 15 bytes
			const PAD: usize = 32 - MSG.len(); // 17 bytes padding to fill the 32-byte word
			let mut data = sp_runtime::Vec::with_capacity(4 + 32 + 32 + 32);
			data.extend_from_slice(&[0x08u8, 0xc3, 0x79, 0xa0]); // Error(string) selector
			data.extend_from_slice(&[0u8; 31]);
			data.push(0x20u8); // string offset = 32
			data.extend_from_slice(&[0u8; 31]);
			data.push(MSG.len() as u8); // string length = 15
			data.extend_from_slice(MSG);
			data.extend_from_slice(&[0u8; PAD]);
			data
		}

		impl<R> pallet_evm::PrecompileSet for BifrostPrecompiles<R>
		where
			R: pallet_evm::Config + pallet_bfc_utility::Config,
			$inner<R>: pallet_evm::PrecompileSet,
			<R as frame_system::Config>::AccountId: From<sp_core::H160>,
		{
			fn execute(
				&self,
				handle: &mut impl pallet_evm::PrecompileHandle,
			) -> Option<pallet_evm::PrecompileResult> {
				let addr = handle.code_address();
				let account: <R as frame_system::Config>::AccountId = addr.into();
				if pallet_bfc_utility::Pallet::<R>::is_blocked_account(&account) {
					return Some(Err(pallet_evm::PrecompileFailure::Revert {
						exit_status: pallet_evm::ExitRevert::Reverted,
						output: blocked_account_revert_data(),
					}));
				}
				self.0.execute(handle)
			}

			fn is_precompile(
				&self,
				address: sp_core::H160,
				gas: u64,
			) -> pallet_evm::IsPrecompileResult {
				let account: <R as frame_system::Config>::AccountId = address.into();
				if pallet_bfc_utility::Pallet::<R>::is_blocked_account(&account) {
					return pallet_evm::IsPrecompileResult::Answer {
						is_precompile: true,
						extra_cost: 0,
					};
				}
				self.0.is_precompile(address, gas)
			}
		}
	};
}
