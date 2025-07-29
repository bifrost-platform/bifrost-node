#[macro_export]
macro_rules! impl_common_runtime_apis {
	{$($custom:tt)*} => {
		use ethereum::AuthorizationList;

		impl_runtime_apis! {
			$($custom)*

			impl sp_api::Core<Block> for Runtime {
				fn version() -> RuntimeVersion {
					VERSION
				}
				fn execute_block(block: Block) {
					Executive::execute_block(block);
				}
				fn initialize_block(header: &<Block as BlockT>::Header) -> ExtrinsicInclusionMode {
					Executive::initialize_block(header)
				}
			}
			impl sp_api::Metadata<Block> for Runtime {
				fn metadata() -> OpaqueMetadata {
					OpaqueMetadata::new(Runtime::metadata().into())
				}
				fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
					Runtime::metadata_at_version(version)
				}
				fn metadata_versions() -> Vec<u32> {
					Runtime::metadata_versions()
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
			impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
				fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
					build_state::<RuntimeGenesisConfig>(config)
				}

				fn get_preset(id: &Option<PresetId>) -> Option<Vec<u8>> {
					get_preset::<RuntimeGenesisConfig>(id, |_| None)
				}

				fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
					vec![]
				}
			}
			impl fp_rpc_debug::DebugRuntimeApi<Block> for Runtime {
				fn trace_transaction(
					extrinsics: Vec<<Block as BlockT>::Extrinsic>,
					traced_transaction: &EthereumTransaction,
					header: &<Block as BlockT>::Header,
				) -> Result<
					(),
					sp_runtime::DispatchError,
				> {
					#[cfg(feature = "evm-tracing")]
					{
						use evm_tracer::tracer::EvmTracer;

						// Initialize block: calls the "on_initialize" hook on every pallet
						// in AllPalletsWithSystem.
						// After pallet message queue was introduced, this must be done only after
						// enabling XCM tracing by setting ETHEREUM_XCM_TRACING_STORAGE_KEY
						// in the storage
						Executive::initialize_block(header);

						// Apply the a subset of extrinsics: all the substrate-specific or ethereum
						// transactions that preceded the requested transaction.
						for ext in extrinsics.into_iter() {
							let _ = match &ext.0.function {
								RuntimeCall::Ethereum(transact { transaction }) => {
									// Reset the previously consumed weight when tracing ethereum transactions.
									// This is necessary because EVM tracing introduces additional
									// (ref_time) overhead, which differs from the production runtime behavior.
									// Without resetting the block weight, the extra tracing overhead could
									// leading to some transactions to incorrectly fail during tracing.
									frame_system::BlockWeight::<Runtime>::kill();

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
					header: &<Block as BlockT>::Header,
				) -> Result<
					(),
					sp_runtime::DispatchError,
				> {
					#[cfg(feature = "evm-tracing")]
					{
						use evm_tracer::tracer::EvmTracer;

						let mut config = <Runtime as pallet_evm::Config>::config().clone();
						config.estimate = true;

						// Initialize block: calls the "on_initialize" hook on every pallet
						// in AllPalletsWithSystem.
						// After pallet message queue was introduced, this must be done only after
						// enabling XCM tracing by setting ETHEREUM_XCM_TRACING_STORAGE_KEY
						// in the storage
						Executive::initialize_block(header);

						// Apply all extrinsics. Ethereum extrinsics are traced.
						for ext in extrinsics.into_iter() {
							match &ext.0.function {
								RuntimeCall::Ethereum(transact { transaction }) => {
									// Reset the previously consumed weight when tracing multiple transactions.
									// This is necessary because EVM tracing introduces additional
									// (ref_time) overhead, which differs from the production runtime behavior.
									// Without resetting the block weight, the extra tracing overhead could
									// leading to some transactions to incorrectly fail during tracing.
									frame_system::BlockWeight::<Runtime>::kill();

									let tx_hash = &transaction.hash();
									if known_transactions.contains(&tx_hash) {
										// Each known extrinsic is a new call stack.
										EvmTracer::emit_new();
										EvmTracer::new().trace(|| {
											if let Err(err) = Executive::apply_extrinsic(ext) {
												log::debug!(
													target: "tracing",
													"Could not trace eth transaction (hash: {}): {:?}",
													&tx_hash,
													err
												);
											}
										});
									} else {
										if let Err(err) = Executive::apply_extrinsic(ext) {
											log::debug!(
												target: "tracing",
												"Failed to apply eth extrinsic (hash: {}): {:?}",
												&tx_hash,
												err
											);
										}
									}
								}
								_ => {
									if let Err(err) = Executive::apply_extrinsic(ext) {
										log::debug!(
											target: "tracing",
											"Failed to apply non-eth extrinsic: {:?}",
											err
										);
									}
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
				fn trace_call(
					header: &<Block as BlockT>::Header,
					from: H160,
					to: H160,
					data: Vec<u8>,
					value: U256,
					gas_limit: U256,
					max_fee_per_gas: Option<U256>,
					max_priority_fee_per_gas: Option<U256>,
					nonce: Option<U256>,
					access_list: Option<Vec<(H160, Vec<H256>)>>,
					authorization_list: Option<AuthorizationList>,
				) -> Result<(), sp_runtime::DispatchError> {
					#[cfg(feature = "evm-tracing")]
					{
						use evm_tracer::tracer::EvmTracer;

						// Initialize block: calls the "on_initialize" hook on every pallet
						// in AllPalletsWithSystem.
						Executive::initialize_block(header);

						EvmTracer::new().trace(|| {
							let is_transactional = false;
							let validate = true;

							let transaction_data = pallet_ethereum::TransactionData::new(
								pallet_ethereum::TransactionAction::Call(to),
								data.clone(),
								nonce.unwrap_or_default(),
								gas_limit,
								None,
								max_fee_per_gas.or(Some(U256::default())),
								max_priority_fee_per_gas.or(Some(U256::default())),
								value,
								Some(<Runtime as pallet_evm::Config>::ChainId::get()),
								access_list.clone().unwrap_or_default(),
								authorization_list.clone().unwrap_or_default(),
							);

							let gas_limit = gas_limit.min(u64::MAX.into()).low_u64();

							let (weight_limit, proof_size_base_cost) = pallet_ethereum::Pallet::<Runtime>::transaction_weight(&transaction_data);

							let _ = <Runtime as pallet_evm::Config>::Runner::call(
								from,
								to,
								data,
								value,
								gas_limit,
								max_fee_per_gas,
								max_priority_fee_per_gas,
								nonce,
								access_list.unwrap_or_default(),
								authorization_list.unwrap_or_default(),
								is_transactional,
								validate,
								weight_limit,
								proof_size_base_cost,
								<Runtime as pallet_evm::Config>::config(),
							);
						});
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
					let (account, _) = pallet_evm::Pallet::<Runtime>::account_basic(&address);
					account
				}
				fn gas_price() -> U256 {
					let (gas_price, _) = <Runtime as pallet_evm::Config>::FeeCalculator::min_gas_price();
					gas_price
				}
				fn account_code_at(address: H160) -> Vec<u8> {
					pallet_evm::AccountCodes::<Runtime>::get(address)
				}
				fn author() -> H160 {
					<pallet_evm::Pallet<Runtime>>::find_author()
				}
				fn storage_at(address: H160, index: U256) -> H256 {
					let mut tmp = [0u8; 32];
					index.to_big_endian(&mut tmp);
					pallet_evm::AccountStorages::<Runtime>::get(address, H256::from_slice(&tmp[..]))
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
					authorization_list: Option<AuthorizationList>,
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

					let transaction_data = pallet_ethereum::TransactionData::new(
						pallet_ethereum::TransactionAction::Call(to),
						data.clone(),
						nonce.unwrap_or_default(),
						gas_limit,
						None,
						max_fee_per_gas.or(Some(U256::default())),
						max_priority_fee_per_gas.or(Some(U256::default())),
						value,
						Some(<Runtime as pallet_evm::Config>::ChainId::get()),
						access_list.clone().unwrap_or_default(),
						authorization_list.clone().unwrap_or_default(),
					);

					let gas_limit = gas_limit.min(u64::MAX.into()).low_u64();
					let (weight_limit, proof_size_base_cost) = pallet_ethereum::Pallet::<Runtime>::transaction_weight(&transaction_data);

					<Runtime as pallet_evm::Config>::Runner::call(
						from,
						to,
						data,
						value,
						gas_limit.unique_saturated_into(),
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						access_list.unwrap_or_default(),
						authorization_list.unwrap_or_default(),
						is_transactional,
						validate,
						weight_limit,
						proof_size_base_cost,
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
					authorization_list: Option<AuthorizationList>,
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
					let transaction_data = pallet_ethereum::TransactionData::new(
						pallet_ethereum::TransactionAction::Create,
						data.clone(),
						nonce.unwrap_or_default(),
						gas_limit,
						None,
						max_fee_per_gas.or(Some(U256::default())),
						max_priority_fee_per_gas.or(Some(U256::default())),
						value,
						Some(<Runtime as pallet_evm::Config>::ChainId::get()),
						access_list.clone().unwrap_or_default(),
						authorization_list.clone().unwrap_or_default(),
					);

					let gas_limit = gas_limit.min(u64::MAX.into()).low_u64();
					let (weight_limit, proof_size_base_cost) = pallet_ethereum::Pallet::<Runtime>::transaction_weight(&transaction_data);

					<Runtime as pallet_evm::Config>::Runner::create(
						from,
						data,
						value,
						gas_limit.unique_saturated_into(),
						max_fee_per_gas,
						max_priority_fee_per_gas,
						nonce,
						access_list.unwrap_or_default(),
						authorization_list.unwrap_or_default(),
						is_transactional,
						validate,
						weight_limit,
						proof_size_base_cost,
						config.as_ref().unwrap_or(<Runtime as pallet_evm::Config>::config()),
					).map_err(|err| err.error.into())
				}
				fn current_transaction_statuses() -> Option<Vec<TransactionStatus>> {
					pallet_ethereum::CurrentTransactionStatuses::<Runtime>::get()
				}
				fn current_block() -> Option<pallet_ethereum::Block> {
					pallet_ethereum::CurrentBlock::<Runtime>::get()
				}
				fn current_receipts() -> Option<Vec<pallet_ethereum::Receipt>> {
					pallet_ethereum::CurrentReceipts::<Runtime>::get()
				}
				fn current_all() -> (
					Option<pallet_ethereum::Block>,
					Option<Vec<pallet_ethereum::Receipt>>,
					Option<Vec<TransactionStatus>>
				) {
					(
						pallet_ethereum::CurrentBlock::<Runtime>::get(),
						pallet_ethereum::CurrentReceipts::<Runtime>::get(),
						pallet_ethereum::CurrentTransactionStatuses::<Runtime>::get()
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
					Some(pallet_base_fee::Elasticity::<Runtime>::get())
				}
				fn gas_limit_multiplier_support() {}
				fn pending_block(
					xts: Vec<<Block as BlockT>::Extrinsic>,
				) -> (Option<pallet_ethereum::Block>, Option<Vec<TransactionStatus>>) {
					for ext in xts.into_iter() {
						let _ = Executive::apply_extrinsic(ext);
					}

					Ethereum::on_finalize(System::block_number() + 1);

					(
						pallet_ethereum::CurrentBlock::<Runtime>::get(),
						pallet_ethereum::CurrentTransactionStatuses::<Runtime>::get()
					)
				}
				fn initialize_pending_block(header: &<Block as BlockT>::Header) {
					Executive::initialize_block(header);
				}
			}
			impl fp_rpc::ConvertTransactionRuntimeApi<Block> for Runtime {
				fn convert_transaction(
					transaction: pallet_ethereum::Transaction,
				) -> <Block as BlockT>::Extrinsic {
					UncheckedExtrinsic::new_bare(
						pallet_ethereum::Call::<Runtime>::transact { transaction }.into(),
					)
				}
			}
			impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
				fn slot_duration() -> sp_consensus_aura::SlotDuration {
					sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
				}
				fn authorities() -> Vec<AuraId> {
					pallet_aura::Authorities::<Runtime>::get().into_inner()
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
			impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
				fn account_nonce(account: AccountId) -> Nonce {
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
