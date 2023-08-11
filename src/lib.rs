use std::io::Read;

use schemars::schema::RootSchema;
use serde::{Serialize, Deserialize};

pub mod prelude {
    pub use serde_json;
    pub use macros::middle_fn;
    pub use crate::{from_host, to_host, FnInfo};
}

/// wasm_alloc is a guest funtion that allocates some new linear memory in the Web Assembly runtime.
/// The host will use the returned pointer to look up the memory that was just set aside, and then fill it with whatever it needs to fill.
#[no_mangle]
pub fn wasm_alloc(size: u32) -> *mut u8 {
    let mut buf: Vec<u8> = Vec::with_capacity(size.try_into().unwrap());
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Transforms an object into some bytes that can then be read by the host.
/// It returns an offset in linear memory, for the host to look up.
/// The `unforget` function must be called after everyone is done with this memory, or else memory usage will grow forever.
/// 
pub fn to_host<T>(obj: &T) -> (*const u8, usize) where T: Sized + serde::Serialize {
    // We need to serialize the object, and postcard seems like a fine way to do this.
    // We'll use Message Pack, which *cross fingers* will allow us to serialize and deserialize objects not known at compile time.
    let bytes: Vec<u8> = rmp_serde::encode::to_vec(obj).expect("to_host: Unable to allocate vector");
    let len = bytes.len();
    let ptr = bytes.as_ptr();

    // This is an important line of code.
    // This will cause Rust to not garbage collect `bytes` at the end of this block.
    // This does mean it's up to the host to call 
    std::mem::forget(ptr);

    (ptr, len)
}

/// "Unforgets" a bit of memory we created for the host.
/// It's important to call this after to_host() is called.
/// It re-constructs a Rust vector, that should have been created earlier by `to_host` and lets it fall out of scope, dropping the value.
#[no_mangle]
pub fn unforget(ptr: *const u8, len: usize) {
    // We're happy this isn't used, we want to drop it.
    let _bytes = unsafe { Vec::from_raw_parts(ptr as *mut u8, len, len) };
}

struct MemoryReclaimer {
    pointer: *mut u8,
    offset: isize,
}

impl MemoryReclaimer {
    fn new(pointer: *mut u8) -> Self {
        Self { pointer, offset: 0 }
    }
}

impl Drop for MemoryReclaimer {
    // Force Rust to free the memory that we've reclaimed, by making a Vec<u8> out of it and allowing it to drop.
    fn drop(&mut self) {
        let offset: usize = self.offset.try_into().unwrap();
        // We're happy this isn't used, we want to drop it.
        let _bytes = unsafe { Vec::from_raw_parts(self.pointer, offset, offset) };
    }
}

impl Read for MemoryReclaimer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        for i in 0..buf.len() {
            // Looking at MessagePack's source code, it looks like it only reads from buffer when it knows there's a value to read from.
            // We shouldn't end up reading linear memory that we're not supposed to read from.
            buf[i] = unsafe { 
                // Copy the value read from memory back into the buffer
                self.pointer.offset(self.offset).read()
            };
            self.offset += 1;
        }
        Ok(buf.len())
    }
}

/// Converts raw bytes from the host back into an value for us to use.
/// `ptr` is, in reality, a simple offset in WASM linear memory, which in this guest code, just looks like the heap.
/// The host has serialized a value of type T into linear memory, and given us that offset with which we should serialize the value once again.
/// 
pub fn from_host<T>(ptr: *mut u8) -> T where T: Sized + serde::de::DeserializeOwned {
    // Use message pack as the serialization library
    let reader = MemoryReclaimer::new(ptr);

    let out: T = rmp_serde::decode::from_read(reader).expect("from_host<T>: error reading from memory");

    out
}

/// Makes a request to an API with the given headers and payload.
/// Returns the status code and body.
pub fn request(input: RequestIn) -> RequestOut {
    let (ptr, len) = to_host(&input);
    let out_ptr = unsafe { host_request(ptr) };
    let out = from_host(out_ptr);
    unforget(ptr, len);
    out
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

#[link(wasm_import_module = "middle")]
extern "C" {
    pub fn host_request(ptr: *const u8) -> *mut u8;
}

#[derive(Serialize)]
pub struct FnInfo {
    pub in_schema: RootSchema,
    pub out_schema: RootSchema,
}