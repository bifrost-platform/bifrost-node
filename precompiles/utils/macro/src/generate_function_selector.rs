use super::*;

pub fn main(_: TokenStream, input: TokenStream) -> TokenStream {
	let item = parse_macro_input!(input as ItemEnum);

	let ItemEnum { attrs, vis, enum_token, ident, variants, .. } = item;

	let mut ident_expressions: Vec<Ident> = vec![];
	let mut variant_expressions: Vec<Expr> = vec![];
	let mut variant_attrs: Vec<Vec<Attribute>> = vec![];
	for variant in variants {
		match variant.discriminant {
			Some((_, Expr::Lit(ExprLit { lit, .. }))) => {
				if let Lit::Str(lit_str) = lit {
					let digest = Keccak256::digest(lit_str.value().as_bytes());
					let selector = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
					ident_expressions.push(variant.ident);
					variant_expressions.push(Expr::Lit(ExprLit {
						lit: Lit::Verbatim(Literal::u32_suffixed(selector)),
						attrs: Default::default(),
					}));
					variant_attrs.push(variant.attrs);
				} else {
					return quote_spanned! {
						lit.span() => compile_error!("Expected literal string");
					}
					.into();
				}
			},
			Some((_eg, expr)) => {
				return quote_spanned! {
					expr.span() => compile_error!("Expected literal");
				}
				.into()
			},
			None => {
				return quote_spanned! {
					variant.span() => compile_error!("Each variant must have a discriminant");
				}
				.into()
			},
		}
	}

	(quote! {
		#(#attrs)*
		#[derive(num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]
		#[repr(u32)]
		#vis #enum_token #ident {
			#(
				#(#variant_attrs)*
				#ident_expressions = #variant_expressions,
			)*
		}
	})
	.into()
}
