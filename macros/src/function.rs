use proc_macro2::{Ident, Span};
use syn::ItemFn;
use quote::quote;
use crate::extract_doc;

/// This macro wraps a user-written function with everything needed for Middle to call it.
/// WebAssembly doesn't let us pass anything other than numbers, so if we want to pass something else, like a string, we have to allocate that string in linear memory and then pass back a pointer and length to the caller.
/// That's what this function does.
/// 
/// In addition, we need user-authored functions to be inspectable by Middle.
/// So, we'll create a second function that outputs that description.
pub fn middle_fn_inner(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let input = syn::parse2::<ItemFn>(input).expect("macro must be a function definition");

    let help_str = extract_doc(input.clone());

    // We want to make it as easy and natural as we can to write and export a Middle function.
    // So, instead of having the user write out a struct for their exported function's inputs and outputs, we'll do that for them.
    // Here we set up variables that are important in the final macro generation.
    let (input_args_sigs, input_args_idents) = {
        let mut in_sig = vec![];
        let mut called_in = vec![];
        input.sig.inputs.iter().for_each(|input| {
            match input {
                syn::FnArg::Receiver(_) => panic!("exported functions must not have `self` as a first argument"),
                syn::FnArg::Typed(p) => {
                    let name = match *p.pat.clone() {
                        syn::Pat::Ident(ident) => ident,
                        _ => panic!("unexpected parameter in function type signature"),
                    };
                    let ty = p.ty.clone();
                    // This will map 
                    //  `foo(a: String, b: u32)`
                    // to
                    //  `a: String`, `b: u32`  
                    in_sig.push(
                        quote! {
                            #name: #ty
                        }
                    );
                    // This will map the above function `foo` to
                    //  `a`, `b`
                    called_in.push(
                        quote! {
                            #name
                        }
                    );
                },
            }
        });
        (in_sig, called_in)
    };

    // Wrap the output of the user's exported function.
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

    // We'll need to wrap function inputs and outputs in a special struct.
    let user_fn_in_struct_ident = Ident::new(&format!("UserFnIn__{}", input.sig.ident), Span::call_site());
    let user_fn_out_struct_ident = Ident::new(&format!("UserFnOut__{}", input.sig.ident), Span::call_site());

    let output = quote! {
        // User's original function, which we leave unchanged.
        // This allows the user to call their own function over again if they like.
        #input

        // Wrap the user's input arguments in a struct that can be taken from the runtime.
        #[derive(Deserialize, JsonSchema)]
        struct #user_fn_in_struct_ident {
            // Map each input to a new member, separated by commas
            #(#input_args_sigs),*
        }

        // Wrap the user's output argument in a struct that can be serialized for consumption by the runtime.
        #[derive(Serialize, JsonSchema)]
        struct #user_fn_out_struct_ident (#out_sig);

        #[no_mangle]
        pub fn #user_fn_name(offset: u32, size: u32) -> u32 {
            // The host calls us with a JSON value.
            // There seems to be no other good way of constructing a value on the host side.
            let input_json: serde_json::Value = value_from_host(offset, size);
            // Convert the JSON value back into a Rust struct.
            let input: #user_fn_in_struct_ident = serde_json::from_value(input_json).expect("user function input could not be serialzied into JSON");
            // Call the user's function.
            let output = #fn_name(
                // Map each input argument identity into (for example) `input.a, input.b, input.c`
                #( input . #input_args_idents ),*
            );
            // Put the user's output in our output struct, which has the serialize derive macro implemented
            let output = #user_fn_out_struct_ident (output);
            // Convert the return value into JSON, so the host can parse it.
            let output_json = serde_json::value::to_value(output).expect("user function output could not be serialized into JSON");
            // Make the result available to the host.
            let (offset, size) = value_to_host(&output_json);
            // Make the offset and size available to the host.
            let offset = vec_parts_to_host(offset, size);
            // All done!
            offset
        }

        #[no_mangle]
        pub fn #introspect_fn_name() -> u32 {
            let fn_info = {
                let in_schema = schemars::schema_for!(#user_fn_in_struct_ident);
                let out_schema = schemars::schema_for!(#user_fn_out_struct_ident);
                let description = #help_str;
                FnInfo {
                    description: description.to_string(), 
                    in_schema, 
                    out_schema
                }
            };
            let (offset, size) = value_to_host(&fn_info);
            let offset = vec_parts_to_host(offset, size);
            offset
        }
    };

    proc_macro2::TokenStream::from(output)
}

mod test {
    use crate::function::*;

    #[test]
    fn test_fn() {
        let generated = middle_fn_inner(
            quote!(
                /// This is my test function
                fn test(a: String, b: u32, c: TestIn) -> Result<(), Error> {
                    Ok(())
                }
            )
        );

        println!("{}", generated);

        let compare = quote!(
            /// This is my test function
            fn test(a: String, b: u32, c: TestIn) -> Result<(), Error> {
                Ok(())
            }

            #[derive(Deserialize, JsonSchema)]
            struct UserFnIn__test {
                a: String,
                b: u32,
                c: TestIn
            }

            #[derive(Serialize, JsonSchema)]
            struct UserFnOut__test(Result<(), Error>);
            
            #[no_mangle]
            pub fn user_fn__test(offset: u32, size: u32) -> u32 {
                let input_json: serde_json::Value = value_from_host(offset, size);
                let input: UserFnIn__test = serde_json::from_value(input_json)
                    .expect("user function input could not be serialzied into JSON");
                let output = test(input.a, input.b, input.c);
                let output = UserFnOut__test(output);
                let output_json = serde_json::value::to_value(output)
                    .expect("user function output could not be serialized into JSON");
                // Hmm. You know, we could try and stuff these two u32s into a i64. 
                let (offset, size) = value_to_host(&output_json);
                let offset = vec_parts_to_host(offset, size);
                offset
            }
            
            #[no_mangle]
            pub fn user_fn_info__test() -> u32 {
                let fn_info = {
                    let in_schema = schemars::schema_for!(UserFnIn__test);
                    let out_schema = schemars::schema_for!(UserFnOut__test);
                    let description = " This is my test function";
                    FnInfo {
                        description: description.to_string(),
                        in_schema,
                        out_schema
                    }
                };
                let (offset, size) = value_to_host(&fn_info);
                let offset = vec_parts_to_host(offset, size);
                offset
            }
        );

        assert_eq!(generated.to_string(), compare.to_string());
    }
}