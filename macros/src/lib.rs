extern crate proc_macro;
extern crate proc_macro2;
use quote::{ quote };
use syn::{ ItemFn };

fn middle_fn_inner(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let input = syn::parse2::<ItemFn>(input).unwrap();

    // First check that the user signature matches what it's supposed to be.
    // fixme: Add more sophisticated valudations
    if input.sig.inputs.len() != 1 {
        panic!("`get_records` must have a single input arg: WasmMainCall");
    }

    // Grab the name of the function.
    // We'll call it twice, so we need to make two clones.
    let fn_name_one = input.sig.ident.clone();
    let fn_name_two = input.sig.ident.clone();

    let output = quote! {
        #[no_mangle]
        pub extern "C" fn #fn_name_one(ptr: *mut u8, len: usize) -> (*const u8, usize) {
            // Rebuild whatever the host called us with.
            let called_with: WasmMainCall = from_host(ptr, len);
        
            // Put in the user's function here.
            #input
        
            // Call the user's function
            let result: WasmMainResult = #fn_name_two(called_with);

            // Make the result available to the host.
            to_host(&result)
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

#[test]
fn test() {
    let generated = middle_fn_inner(
        quote!(
            fn hello(input: WasmMainCall) -> WasmMainResult {
                WasmMainResult { }
            }
        )
    );

    // println!("{}", generated);

    let compare = quote!(
        # [no_mangle] 
        pub extern "C" fn hello (ptr : * mut u8 , len : usize) -> (* const u8 , usize) { 
            let called_with : WasmMainCall = from_host (ptr , len) ; 
            fn hello (input : WasmMainCall) -> WasmMainResult { WasmMainResult { } } 
            let result : WasmMainResult = hello (called_with) ; 
            to_host (& result) 
        }        
    );

    assert_eq!(generated.to_string(), compare.to_string());
}