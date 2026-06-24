fn main() {
	#[cfg(all(feature = "std", feature = "metadata-hash"))]
	substrate_wasm_builder::WasmBuilder::init_with_defaults()
		.enable_metadata_hash("BFC", 18)
		.build();

	#[cfg(all(feature = "std", not(feature = "metadata-hash")))]
	substrate_wasm_builder::WasmBuilder::build_using_defaults();
}
