#![crate_type = "proc-macro"]
extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::{quote, quote_spanned};
use sha3::{Digest, Keccak256};
use syn::{
	parse_macro_input, spanned::Spanned, Attribute, Expr, ExprLit, Ident, ItemEnum, Lit, LitStr,
};

mod generate_function_selector;
mod precompile;

struct Bytes(Vec<u8>);

impl ::std::fmt::Debug for Bytes {
	#[inline]
	fn fmt(&self, f: &mut std::fmt::Formatter) -> ::std::fmt::Result {
		let data = &self.0;
		write!(f, "[")?;
		if !data.is_empty() {
			write!(f, "{:#04x}u8", data[0])?;
			for unit in data.iter().skip(1) {
				write!(f, ", {:#04x}", unit)?;
			}
		}
		write!(f, "]")
	}
}

#[proc_macro]
pub fn keccak256(input: TokenStream) -> TokenStream {
	let lit_str = parse_macro_input!(input as LitStr);

	let hash = Keccak256::digest(lit_str.value().as_bytes());

	let bytes = Bytes(hash.to_vec());
	let eval_str = format!("{:?}", bytes);
	let eval_ts: proc_macro2::TokenStream = eval_str.parse().unwrap_or_else(|_| {
		panic!("Failed to parse the string \"{}\" to TokenStream.", eval_str);
	});
	quote!(#eval_ts).into()
}

/// This macro allows to associate to each variant of an enumeration a discriminant (of type u32
/// whose value corresponds to the first 4 bytes of the Hash Keccak256 of the character string
///indicated by the user of this macro.
///
/// Usage:
///
/// ```ignore
/// #[generate_function_selector]
/// enum Action {
/// 	Toto = "toto()",
/// 	Tata = "tata()",
/// }
/// ```
///
/// Extended to:
///
/// ```rust
/// #[repr(u32)]
/// enum Action {
/// 	Toto = 119097542u32,
/// 	Tata = 1414311903u32,
/// }
/// ```
#[proc_macro_attribute]
pub fn generate_function_selector(attr: TokenStream, input: TokenStream) -> TokenStream {
	generate_function_selector::main(attr, input)
}

#[proc_macro_attribute]
pub fn precompile(attr: TokenStream, input: TokenStream) -> TokenStream {
	precompile::main(attr, input)
}
