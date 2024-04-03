fn main() {
	#[cfg(feature = "std")]
	{
		std::env::set_var("CFLAGS", "-mcpu=mvp");
		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.build();
	}
}
