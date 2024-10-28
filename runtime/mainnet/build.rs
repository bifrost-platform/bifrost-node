fn main() {
	#[cfg(feature = "std")]
	{
		#[cfg(target_arch = "aarch64")]
		std::env::set_var("CFLAGS", "-mcpu=mvp");

		substrate_wasm_builder::WasmBuilder::build_using_defaults();
	}
}
