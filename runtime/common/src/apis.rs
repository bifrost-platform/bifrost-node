#[macro_export]
macro_rules! impl_common_runtime_apis {
	{$($custom:tt)*} => {
		impl_runtime_apis! {
			$($custom)*

			impl sp_api::Core<Block> for Runtime {
				fn version() -> RuntimeVersion {
					VERSION
				}
				fn execute_block(block: Block) {
					Executive::execute_block(block);
				}
				fn initialize_block(header: &<Block as BlockT>::Header) {
					Executive::initialize_block(header)
				}
			}
			impl sp_api::Metadata<Block> for Runtime {
				fn metadata() -> OpaqueMetadata {
					OpaqueMetadata::new(Runtime::metadata().into())
				}
			}
			impl sp_block_builder::BlockBuilder<Block> for Runtime {
				fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
					Executive::apply_extrinsic(extrinsic)
				}
				fn finalize_block() -> <Block as BlockT>::Header {
					Executive::finalize_block()
				}
				fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
					data.create_extrinsics()
				}
				fn check_inherents(
					block: Block,
					data: sp_inherents::InherentData,
				) -> sp_inherents::CheckInherentsResult {
					data.check_extrinsics(&block)
				}
			}
			impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
				fn validate_transaction(
					source: TransactionSource,
					tx: <Block as BlockT>::Extrinsic,
					block_hash: <Block as BlockT>::Hash,
				) -> TransactionValidity {
					Executive::validate_transaction(source, tx, block_hash)
				}
			}
			impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
				fn offchain_worker(header: &<Block as BlockT>::Header) {
					Executive::offchain_worker(header)
				}
			}
			impl fp_rpc_debug::DebugRuntimeApi<Block> for Runtime {
				fn trace_transaction(
					extrinsics: Vec<<Block as BlockT>::Extrinsic>,
					traced_transaction: &EthereumTransaction,
				) -> Result<
					(),
					sp_runtime::DispatchError,
				> {
					#[cfg(feature = "evm-tracing")]
					{
						use evm_tracer::tracer::EvmTracer;
						// Apply the a subset of extrinsics: all the substrate-specific or ethereum
						// transactions that preceded the requested transaction.
						for ext in extrinsics.into_iter() {
							let _ = match &ext.0.function {
								RuntimeCall::Ethereum(transact { transaction }) => {
									if transaction == traced_transaction {
										EvmTracer::new().trace(|| Executive::apply_extrinsic(ext));
										return Ok(());
									} else {
										Executive::apply_extrinsic(ext)
									}
								}
								_ => Executive::apply_extrinsic(ext),
							};
						}
						Err(sp_runtime::DispatchError::Other(
							"Failed to find Ethereum transaction among the extrinsics.",
						))
					}
					#[cfg(not(feature = "evm-tracing"))]
					Err(sp_runtime::DispatchError::Other(
						"Missing `evm-tracing` compile time feature flag.",
					))
				}
				fn trace_block(
					extrinsics: Vec<<Block as BlockT>::Extrinsic>,
					known_transactions: Vec<H256>,
				) -> Result<
					(),
					sp_runtime::DispatchError,
				> {
					#[cfg(feature = "evm-tracing")]
					{
						use evm_tracer::tracer::EvmTracer;
						let mut config = <Runtime as pallet_evm::Config>::config().clone();
						config.estimate = true;
						// Apply all extrinsics. Ethereum extrinsics are traced.
						for ext in extrinsics.into_iter() {
							match &ext.0.function {
								RuntimeCall::Ethereum(transact { transaction }) => {
									if known_transactions.contains(&transaction.hash()) {
										// Each known extrinsic is a new call stack.
										EvmTracer::emit_new();
										EvmTracer::new().trace(|| Executive::apply_extrinsic(ext));
									} else {
										let _ = Executive::apply_extrinsic(ext);
									}
								}
								_ => {
									let _ = Executive::apply_extrinsic(ext);
								}
							};
						}
						Ok(())
					}
					#[cfg(not(feature = "evm-tracing"))]
					Err(sp_runtime::DispatchError::Other(
						"Missing `evm-tracing` compile time feature flag.",
					))
				}
			}
			impl fp_rpc_txpool::TxPoolRuntimeApi<Block> for Runtime {
				fn extrinsic_filter(
					xts_ready: Vec<<Block as BlockT>::Extrinsic>,
					xts_future: Vec<<Block as BlockT>::Extrinsic>,
				) -> TxPoolResponse {
					TxPoolResponse {
						ready: xts_ready
						.into_iter()
						.filter_map(|xt| match xt.0.function {
							RuntimeCall::Ethereum(transact { transaction }) => Some(transaction),
							_ => None,
						})
						.collect(),
						future: xts_future
						.into_iter()
						.filter_map(|xt| match xt.0.function {
							RuntimeCall::Ethereum(transact { transaction }) => Some(transaction),
							_ => None,
						})
						.collect(),
					}
				}
			}
			impl fp_rpc::EthereumRuntimeRPCApi<Block> for Runtime {
				fn chain_id() -> u64 {
					<Runtime as pallet_evm::Config>::ChainId::get()
				}
				fn account_basic(address: H160) -> EVMAccount {
					let (account, _) = EVM::account_basic(&address);
					account
				}
				fn gas_price() -> U256 {
					let (gas_price, _) = <Runtime as pallet_evm::Config>::FeeCalculator::min_gas_price();
					gas_price
				}
				fn account_code_at(address: H160) -> Vec<u8> {
					EVM::account_codes(address)
				}
				fn author() -> H160 {
					<pallet_evm::Pallet<Runtime>>::find_author()
				}
				fn storage_at(address: H160, index: U256) -> H256 {
					let mut tmp = [0u8; 32];
					index.to_big_endian(&mut tmp);
					EVM::account_storages(address, H256::from_slice(&tmp[..]))
				}
				fn call(
					from: H160,
					to: H160,
					data: Vec<u8>,
					value: U256,
					gas_limit: U256,
					max_fee_per_gas: Option<U256>,
					max_priority_fee_per_gas: Option<U256>,
					nonce: Option<U256>,
					estimate: bool,
					access_list: Option<Vec<(H160, Vec<H256>)>>,
				) -> Result<pallet_evm::CallInfo, sp_runtime::DispatchError> {
					let config = if estimate {
						let mut config = <Runtime as pallet_evm::Config>::config().clone();
						config.estimate = true;
						Some(config)
					} else {
						None
					};
					let is_transactional = false;
					let validate = true;
					<Runtime as pallet_evm::Config>::Runner::call(
						from,
						to,
						data,
						value,
						gas_limit.low_u64(),
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						access_list.unwrap_or_default(),
						is_transactional,
						validate,
						config.as_ref().unwrap_or(<Runtime as pallet_evm::Config>::config()),
					).map_err(|err| err.error.into())
				}
				fn create(
					from: H160,
					data: Vec<u8>,
					value: U256,
					gas_limit: U256,
					max_fee_per_gas: Option<U256>,
					max_priority_fee_per_gas: Option<U256>,
					nonce: Option<U256>,
					estimate: bool,
					access_list: Option<Vec<(H160, Vec<H256>)>>,
				) -> Result<pallet_evm::CreateInfo, sp_runtime::DispatchError> {
					let config = if estimate {
						let mut config = <Runtime as pallet_evm::Config>::config().clone();
						config.estimate = true;
						Some(config)
					} else {
						None
					};
					let is_transactional = false;
					let validate = true;
					<Runtime as pallet_evm::Config>::Runner::create(
						from,
						data,
						value,
						gas_limit.low_u64(),
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						access_list.unwrap_or_default(),
						is_transactional,
						validate,
						config.as_ref().unwrap_or(<Runtime as pallet_evm::Config>::config()),
					).map_err(|err| err.error.into())
				}
				fn current_transaction_statuses() -> Option<Vec<TransactionStatus>> {
					Ethereum::current_transaction_statuses()
				}
				fn current_block() -> Option<pallet_ethereum::Block> {
					Ethereum::current_block()
				}
				fn current_receipts() -> Option<Vec<pallet_ethereum::Receipt>> {
					Ethereum::current_receipts()
				}
				fn current_all() -> (
					Option<pallet_ethereum::Block>,
					Option<Vec<pallet_ethereum::Receipt>>,
					Option<Vec<TransactionStatus>>
				) {
					(
						Ethereum::current_block(),
						Ethereum::current_receipts(),
						Ethereum::current_transaction_statuses()
					)
				}
				fn extrinsic_filter(
					xts: Vec<<Block as BlockT>::Extrinsic>,
				) -> Vec<EthereumTransaction> {
					xts.into_iter().filter_map(|xt| match xt.0.function {
						RuntimeCall::Ethereum(transact{transaction}) => Some(transaction),
						_ => None
					}).collect::<Vec<EthereumTransaction>>()
				}
				fn elasticity() -> Option<Permill> {
					Some(BaseFee::elasticity())
				}
				fn gas_limit_multiplier_support() {}
			}
			impl fp_rpc::ConvertTransactionRuntimeApi<Block> for Runtime {
				fn convert_transaction(
					transaction: pallet_ethereum::Transaction,
				) -> <Block as BlockT>::Extrinsic {
					UncheckedExtrinsic::new_unsigned(
						pallet_ethereum::Call::<Runtime>::transact { transaction }.into(),
					)
				}
			}
			impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
				fn slot_duration() -> sp_consensus_aura::SlotDuration {
					sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
				}
				fn authorities() -> Vec<AuraId> {
					Aura::authorities().into_inner()
				}
			}
			impl sp_session::SessionKeys<Block> for Runtime {
				fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
					opaque::SessionKeys::generate(seed)
				}
				fn decode_session_keys(
					encoded: Vec<u8>,
				) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
					opaque::SessionKeys::decode_into_raw_public_keys(&encoded)
				}
			}
			impl fg_primitives::GrandpaApi<Block> for Runtime {
				fn grandpa_authorities() -> GrandpaAuthorityList {
					Grandpa::grandpa_authorities()
				}
				fn current_set_id() -> fg_primitives::SetId {
					Grandpa::current_set_id()
				}
				fn submit_report_equivocation_unsigned_extrinsic(
					_equivocation_proof: fg_primitives::EquivocationProof<
						<Block as BlockT>::Hash,
						NumberFor<Block>,
					>,
					_key_owner_proof: fg_primitives::OpaqueKeyOwnershipProof,
				) -> Option<()> {
					None
				}
				fn generate_key_ownership_proof(
					_set_id: fg_primitives::SetId,
					_authority_id: GrandpaId,
				) -> Option<fg_primitives::OpaqueKeyOwnershipProof> {
					None
				}
			}
			impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
				fn account_nonce(account: AccountId) -> Index {
					System::account_nonce(account)
				}
			}
			impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
				fn query_info(
					uxt: <Block as BlockT>::Extrinsic,
					len: u32,
				) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
					TransactionPayment::query_info(uxt, len)
				}
				fn query_fee_details(
					uxt: <Block as BlockT>::Extrinsic,
					len: u32,
				) -> pallet_transaction_payment::FeeDetails<Balance> {
					TransactionPayment::query_fee_details(uxt, len)
				}
				fn query_weight_to_fee(weight: Weight) -> Balance {
					TransactionPayment::weight_to_fee(weight)
				}
				fn query_length_to_fee(length: u32) -> Balance {
					TransactionPayment::length_to_fee(length)
				}
			}
			#[cfg(feature = "runtime-benchmarks")]
			impl frame_benchmarking::Benchmark<Block> for Runtime {
				fn benchmark_metadata(extra: bool) -> (
					Vec<frame_benchmarking::BenchmarkList>,
					Vec<frame_support::traits::StorageInfo>,
				) {
					use frame_benchmarking::{baseline, Benchmarking, BenchmarkList};
					use frame_support::traits::StorageInfoTrait;
					use frame_system_benchmarking::Pallet as SystemBench;
					use baseline::Pallet as BaselineBench;
					let mut list = Vec::<BenchmarkList>::new();
					list_benchmarks!(list, extra);
					let storage_info = AllPalletsWithSystem::storage_info();
					return (list, storage_info)
				}
				fn dispatch_benchmark(
					config: frame_benchmarking::BenchmarkConfig
				) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
					use frame_benchmarking::{baseline, Benchmarking, BenchmarkBatch, TrackedStorageKey};
					use frame_system_benchmarking::Pallet as SystemBench;
					use baseline::Pallet as BaselineBench;
					impl frame_system_benchmarking::Config for Runtime {}
					impl baseline::Config for Runtime {}
					let whitelist: Vec<TrackedStorageKey> = vec![
						// Block Number
						hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac").to_vec().into(),
						// Total Issuance
						hex_literal::hex!("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80").to_vec().into(),
						// Execution Phase
						hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a").to_vec().into(),
						// Event Count
						hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850").to_vec().into(),
						// System Events
						hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7").to_vec().into(),
					];
					let mut batches = Vec::<BenchmarkBatch>::new();
					let params = (&config, &whitelist);
					add_benchmarks!(params, batches);
					Ok(batches)
				}
			}
		}
	};
}
