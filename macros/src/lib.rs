use syn::ItemFn;

extern crate proc_macro;
extern crate proc_macro2;

mod workflow;
mod function;

/// Copies the "doc" attribute of a function.
/// This is the triple-/ comment block that actually becomes a #[doc=""] attribute.
fn extract_doc(input: ItemFn) -> String {
    let help_str = {
        let out = input.attrs.iter().find_map(|attr| {
            if attr.path().is_ident("doc") {
                match &attr.meta {
                    syn::Meta::NameValue(value) => {
                        if let syn::Expr::Lit(lit) = &value.value {
                            if let syn::Lit::Str(s) = &lit.lit {
                                return Some(s.value())
                            }
                        }
                    },
                    _ => panic!("Invalid doc attribute"),
                }
            }
            None
        });
        match out {
            Some(doc) => doc,
            None => "".to_string(),
        }
    };
    help_str
}

#[proc_macro_attribute]
pub fn middle_fn(_attr: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let output: proc_macro2::TokenStream = function::middle_fn_inner(input.into());
    proc_macro::TokenStream::from(output)
}

#[proc_macro_attribute]
pub fn middle_workflow(_attr: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let output = workflow::middle_workflow_inner(input.into());
    proc_macro::TokenStream::from(output)
}
