use syn::ItemFn;

extern crate proc_macro;
extern crate proc_macro2;

mod multistep_function;
mod function;

/// Copies the "doc" attribute of a function.
/// This is the triple-/ comment block that actually becomes a #[doc=""] attribute.
fn extract_doc(input: ItemFn) -> String {
    let help_str = {
        let out = input.attrs.iter().filter_map(|attr| {
            if attr.path().is_ident("doc") {
                match &attr.meta {
                    syn::Meta::NameValue(value) => {
                        if let syn::Expr::Lit(lit) = &value.value {
                            if let syn::Lit::Str(s) = &lit.lit {
                                let val = s.value().trim().to_string();
                                return Some(val)
                            }
                        }
                    },
                    _ => panic!("Invalid doc attribute"),
                }
            }
            None
        });
        let out: Vec<_> = out.into_iter().collect();
        let out = out.join("\n");
        out
    };
    help_str
}

#[proc_macro_attribute]
pub fn middle_fn(_attr: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let output: proc_macro2::TokenStream = function::middle_fn_inner(input.into());
    proc_macro::TokenStream::from(output)
}

#[proc_macro_attribute]
pub fn middle_multistep_fn(_attr: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let output = multistep_function::middle_multistep_function_inner(input.into());
    proc_macro::TokenStream::from(output)
}
