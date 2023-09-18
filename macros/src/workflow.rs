use proc_macro2::{Ident, Span};
use syn::ItemFn;

use quote::quote;

use crate::extract_doc;


pub fn middle_workflow_inner(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let input: ItemFn = syn::parse2::<ItemFn>(input).expect("macro must be a function definition");

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
    // Make sure the function returns Resumable<>, and extract the inside of the angle brackets.
    let out_sig = match input.sig.output.clone() {
        syn::ReturnType::Default => panic!("exported functions must have an explicit return type"),
        syn::ReturnType::Type(_, t) => {
            let t = (*t).clone();
            if let syn::Type::Path(path) = t {
                let seg = match path.path.segments.iter().last() {
                    Some(seg) => seg,
                    None => panic!("Return type missing path. Workflows must return Resumable<...>"),
                };
                if seg.ident.to_owned() == "Resumable" {
                    match &seg.arguments {
                        syn::PathArguments::AngleBracketed(contained) => {
                            if contained.args.len() == 1 {
                                let first = &contained.args[0];
                                match first {
                                    syn::GenericArgument::Type(t) => {
                                        (*t).clone()
                                    },
                                    _ => panic!(". Workflows must return Resumable<...>"),
                                }
                            } else {
                                panic!("Resumable<T> must be called with a single argument. Workflows must return Resumable<...>");
                            }
                        },
                        _ => panic!("Resumable<T> must be called with angle brackets. Workflows must return Resumable<...>"),
                    }
                } else {
                    panic!("Incorrect return type. Workflows must return Resumable<...>")
                }
            } else {
                panic!("Return type is unexpected. Workflows must return Resumable<...>")
            }
        },
    };

    // Generate the wrapped name of the function.
    // Prefix it to help identify it later.
    let user_fn_name = Ident::new(&format!("user_workflow__{}", input.sig.ident), Span::call_site());

    // Create a second function which we'll use to output the signature of the user-written function.
    // Prefix this one as well to help identify later.
    let introspect_fn_name = Ident::new(&format!("user_workflow_info__{}", input.sig.ident), Span::call_site());

    // We have to reassign/clone the original fn ident for Rust to like our macro.
    let fn_name = input.sig.ident.clone();

    // We'll need to wrap function inputs and outputs in a special struct.
    let user_fn_in_struct_ident = Ident::new(&format!("UserWorkflowIn__{}", input.sig.ident), Span::call_site());
    let user_fn_out_struct_ident = Ident::new(&format!("UserWorkflowOut__{}", input.sig.ident), Span::call_site());

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
            let input: #user_fn_in_struct_ident = serde_json::from_value(input_json).expect("user workflow input could not be serialzied into JSON");
            // Call the user's function.
            let output = #fn_name(
                // Map each input argument identity into (for example) `input.a, input.b, input.c`
                #( input . #input_args_idents ),*
            );
            // Convert the return value into JSON, so the host can parse it.
            let output_json = serde_json::value::to_value(output).expect("user workflow output could not be serialized into JSON");
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
    use crate::workflow::*;

    #[test]
    fn test_workflow() {
        let generated = middle_workflow_inner(
            quote!(
                /// This is my test workflow
                /// Second line of test function
                fn test(a: String, b: u32, c: TestIn) -> Resumable<Result<(), Error> > {
                    Resumable::Ready(Ok(()))
                }
            ),
        );

        println!("{}", generated);

        let compare = quote!(
            /// This is my test workflow
            /// Second line of test function
            fn test(a: String, b: u32, c: TestIn) -> Resumable<Result<(), Error> > {
                Resumable::Ready(Ok(()))
            }

            #[derive(Deserialize, JsonSchema)]
            struct UserWorkflowIn__test {
                a: String,
                b: u32,
                c: TestIn
            }

            #[derive(Serialize, JsonSchema)]
            struct UserWorkflowOut__test(Result<(), Error>);
            
            #[no_mangle]
            pub fn user_workflow__test(offset: u32, size: u32) -> u32 {
                let input_json: serde_json::Value = value_from_host(offset, size);
                let input: UserWorkflowIn__test = serde_json::from_value(input_json)
                    .expect("user workflow input could not be serialzied into JSON");
                let output = test(input.a, input.b, input.c);
                let output_json = serde_json::value::to_value(output)
                    .expect("user workflow output could not be serialized into JSON");
                // Hmm. You know, we could try and stuff these two u32s into a i64. 
                let (offset, size) = value_to_host(&output_json);
                let offset = vec_parts_to_host(offset, size);
                offset
            }
            
            #[no_mangle]
            pub fn user_workflow_info__test() -> u32 {
                let fn_info = {
                    let in_schema = schemars::schema_for!(UserWorkflowIn__test);
                    let out_schema = schemars::schema_for!(UserWorkflowOut__test);
                    let description = "This is my test workflow\nSecond line of test function";
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