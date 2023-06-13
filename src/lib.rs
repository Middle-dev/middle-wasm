use serde::{Serialize, Deserialize};

/// `walloc` has the guest allocate some memory in a vector from within the guest.
/// This memory is created within the linear memory of the WASM runtime.
/// The host will use the offset to look up the memory that was just set aside, and then fill it with whatever it needs to fill.
#[no_mangle]
pub extern "C" fn wasm_alloc(size: u32) -> *mut u8 {
    let mut buf: Vec<u8> = Vec::with_capacity(size.try_into().unwrap());
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum In {
    Request(String),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum Out {
    Ok(u32),
    Error(String)
}

/// The host always calls main with this type of object.
/// 
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct WasmMainCall {

}

/// The host requires us to return this type of object. 
/// 
#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum WasmMainResult {
    Error(String),
    Ok
}

/// Transforms an object into some bytes that then can be read by the host.
/// Beware of memory leaks!
/// This function creates a new vector and then never calls the destructor on it.
/// Right now, 
/// 
fn to_host<T>(obj: &T) -> (*const u8, usize) where T: Sized + serde::Serialize {
    let out = postcard::to_stdvec(obj).unwrap();

    let ptr = out.as_ptr();
    let len = out.len();
    std::mem::forget(out);

    (ptr, len)
}

/// "Unforgets" a bit of memory we created for the host.
/// It's important to call this after to_host() is called.
/// It re-constructs a Rust vector, that should have been created earlier by `to_host` and lets it fall out of scope, dropping the value.
/// 
fn unforget(ptr: *const u8, len: usize) -> () {
    let _out = unsafe { Vec::from_raw_parts(ptr as *mut u8, len, len) };
}

/// Converts raw bytes (passed as a pointer) from the host back into an value for us to use.
/// 
fn from_host<T>(ptr: *mut u8, len: usize) -> T where T: Sized + serde::de::DeserializeOwned {
    // First convert the offset and len back back into a vector.
    let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };

    // Now decode it.
    let out: T = postcard::from_bytes(&bytes[..]).unwrap();
    out
}

/// Called by the host as the main extrypoint into this WASM module.
/// 
#[no_mangle]
pub extern "C" fn main(ptr: *mut u8, len: usize) -> (*const u8, usize) {
    // Reconstruct whatever the host gave us.
    let called_with: WasmMainCall = from_host(ptr, len);

    // Here we should call user code... Let's just stub this out for now.
    let user_code = |input: WasmMainCall| -> WasmMainResult {
        // Make a dummy api call
        let response = request(RequestIn { url: "http://asdfasdfasdf.com".to_string() });

        if response.http_code != 200 {
            WasmMainResult::Error(format!("got error: {response:?}").to_string())
        } else {
            WasmMainResult::Ok
        }
    };
    let result = user_code(called_with);
    to_host(&result)
}

/// Makes a request to an API with the given headers and payload.
/// Returns the status code and body.
pub fn request(input: RequestIn) -> RequestOut {
    let (offset, len) = to_host(&input);
    let (out_ptr, out_len) = unsafe { host_request(offset, len) };
    unforget(offset, len);
    from_host(out_ptr, out_len)
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RequestIn {
    url: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RequestOut {
    http_code: u32,
    body: String
}

extern "C" {
    fn host_request(ptr: *const u8, len: usize) -> (*mut u8, usize);
}