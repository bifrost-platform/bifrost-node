fn main() {
	#[cfg(feature = "std")]
	{
		#[cfg(target_arch = "aarch64")]
		std::env::set_var("CFLAGS", "-mcpu=mvp");

		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.build();
	}
}
