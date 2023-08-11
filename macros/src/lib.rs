extern crate proc_macro;
extern crate proc_macro2;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::ItemFn;

/// This macro wraps a user-written function with everything needed for Middle to call it.
/// WebAssembly doesn't let us pass anything other than numbers, so if we want to pass something else, like a string, we have to allocate that string in linear memory and then pass back a pointer and length to the caller.
/// That's what this function does.
/// 
/// In addition, we need user-authored functions to be inspectable by Middle.
/// So, we'll create a second function that outputs that description.
/// 
fn middle_fn_inner(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let input = syn::parse2::<ItemFn>(input).unwrap();

    // First check that the user signature matches what it's supposed to be.
    // fixme: Add more sophisticated valudations
    if input.sig.inputs.len() != 1 {
        panic!("exported functions must have a single input arg, and it must to implement JsonSchema");
    }

    let in_sig = match input.sig.inputs[0].clone() {
        syn::FnArg::Receiver(_) => panic!("exported functions must not have `self` as a first argument"),
        syn::FnArg::Typed(p) => {
            p.ty
        },
    };

    let out_sig = match input.sig.output.clone() {
        syn::ReturnType::Default => panic!("exported functions must have an explicit return type"),
        syn::ReturnType::Type(_, t) => t,
    };

    // Generate the wrapped name of the function.
    // Prefix it to help identify it later.
    let user_fn_name = Ident::new(&format!("user_fn__{}", input.sig.ident), Span::call_site());

    // Create a second function which we'll use to output the signature of the user-written function.
    // Prefix this one as well to help identify later.
    let introspect_fn_name = Ident::new(&format!("user_fn_info__{}", input.sig.ident), Span::call_site());

    // We have to reassign/clone the original fn ident for Rust to like our macro.
    let fn_name = input.sig.ident.clone();

    let output = quote! {
        // User's original function, which we leave unchanged.
        // This allows the user to call their own function over again if they like.
        #input

        #[no_mangle]
        pub fn #user_fn_name(ptr: *mut u8) -> *const u8 {
            // The host calls us with a JSON value.
            // There seems to be no other good way of constructing a value on the host side.
            let input_json: serde_json::Value = from_host(ptr);
            // Convert the JSON value back into a Rust struct.
            let input: #in_sig = serde_json::from_value(input_json).expect("user function input could not be serialzied into JSON");
            // Call the user's function.
            let output = #fn_name(input);
            // Convert the return value into JSON, so the host can parse it.
            let output_json = serde_json::value::to_value(output).expect("user function output could not be serialized into JSON");
            // Make the result available to the host.
            let ptr = to_host(&output_json);
            ptr
        }

        #[no_mangle]
        pub fn #introspect_fn_name() -> *const u8 {
            let fn_info = {
                let in_schema = schemars::schema_for!(#in_sig);
                let out_schema = schemars::schema_for!(#out_sig);
                FnInfo {
                    in_schema, out_schema
                }
            };
            let ptr = to_host(&fn_info);
            ptr
        }
    };

    proc_macro2::TokenStream::from(output)
}

#[proc_macro_attribute]
pub fn middle_fn(_attr: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    let output: proc_macro2::TokenStream = middle_fn_inner(input);
    proc_macro::TokenStream::from(output)
}

mod test {
    use crate::*;

    #[test]
    fn test() {
        let generated = middle_fn_inner(
            quote!(
                fn test(input: TestIn) -> TestOut {
                    println!("This is my function!");
                    TestOut {
                        my_str: format!("I was given {}", input.my_str),
                        my_num: input.my_num + 1,
                    }
                }
            )
        );

        println!("{}", generated);

        let compare = quote!(
            fn test(input: TestIn) -> TestOut {
                println!("This is my function!");
                TestOut {
                    my_str: format!("I was given {}", input.my_str),
                    my_num: input.my_num + 1,
                }
            }
            
            #[no_mangle]
            pub fn user_fn__test(ptr: *mut u8) -> *const u8 {
                let input_json: serde_json::Value = from_host(ptr);
                let input: TestIn = serde_json::from_value(input_json).expect("user function input could not be serialzied into JSON");
                let output = test(input);
                let output_json = serde_json::value::to_value(output).expect("user function output could not be serialized into JSON");
                let ptr = to_host(&output_json);
                ptr
            }
            
            #[no_mangle]
            pub fn user_fn_info__test() -> *const u8 {
                let fn_info = {
                    let in_schema = schemars::schema_for!(TestIn);
                    let out_schema = schemars::schema_for!(TestOut);
                    FnInfo {
                        in_schema,
                        out_schema
                    }
                };
                let ptr = to_host(&fn_info);
                ptr
            }
            
        );

        assert_eq!(generated.to_string(), compare.to_string());
    }
}