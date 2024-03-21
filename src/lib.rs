#![feature(vec_into_raw_parts)]
#![feature(try_trait_v2)]
#![feature(panic_hooks)]

use std::{time::Duration, ops::{Try, ControlFlow, FromResidual}, convert};

use schemars::schema::RootSchema;
use serde::{Serialize, Deserialize};

mod request;
mod prompt;

pub use request::{HostRequestResponse, request, RequestBuilder};
pub use prompt::{prompt, prompt_with_schema};

pub mod prelude {
    // All of these exports are needed for the #[middle_fn()] macro to work
    pub use macros::{middle_fn, middle_multistep_fn};
    pub use serde_json;
    pub use serde::{Serialize, Deserialize};
    pub use schemars::JsonSchema;
    pub use crate::{value_from_host, value_to_host, vec_parts_to_host, FnInfo, Resumable, mprint};
    pub use crate::{HostRequestResponse, request, RequestBuilder};
    pub use crate::{prompt, prompt_with_schema};

}

use std::panic;

/// A guest function that should be called once per instantiation, and sets up WASM so that
/// if a panic occurs in code, we get a printout of it.
#[no_mangle]
pub fn setup() {
    std::panic::set_hook(Box::new(|info| {

        let file = info.location().unwrap().file();
        let line = info.location().unwrap().line();
        let col = info.location().unwrap().column();

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => {
                match info.payload().downcast_ref::<String>() {
                    Some(s) => &s[..],
                    None => "Box<Any>",
                }
            }
        };

        let err_info = format!("Panicked at '{}', {}:{}:{}", msg, file, line, col);
        let (offset, len) = value_to_host(&err_info);
        unsafe { host_panic(offset, len);  }
    }));
}

/// A guest function allocating linear memory in the Web Assembly runtime for use by the host.
/// The host will use the returned pointer to look up the memory that was just set aside, and then fill it with whatever it needs to fill.
#[no_mangle]
pub fn wasm_alloc(len: u32) -> u32 {
    let buf: Vec<u8> = Vec::with_capacity(len.try_into().unwrap());
    let (ptr, len, cap) = buf.into_raw_parts();
    let offset = ptr as u32;
    println!("GUEST: wasm_alloc, created with offset={offset}, len={len}, cap={cap}");
    offset
}

/// Transforms an object into a vector that can then be read by the host.
/// Returns the offset in linear memory starting the vector, plus its length and capacity, which are needed to reconstruct and then call the destructor on this vector later.
pub fn value_to_host<T>(obj: &T) -> (u32, u32) where T: Sized + serde::Serialize {
    // We need to serialize the object, and postcard seems like a fine way to do this.
    // We'll use Message Pack, which should allow us to serialize and deserialize objects not known at compile time.
    // There's an alternative to `to_vec` which retains key order, but I don't think it's needed, as we'll always serialize user values into serde_json::Value.
    let bytes: Vec<u8> = rmp_serde::encode::to_vec(obj).expect("to_host: Unable to allocate vector");
    
    // This is an important line of code.
    // This will cause Rust to not garbage collect `bytes` at the end of this block.
    // This does mean it's up to the host to call `unforget` on the reconstructed pointer
    let (ptr, _len, cap) = bytes.into_raw_parts();

    let (offset, size) = (ptr as u32, cap as u32);
    println!("GUEST: value_to_host, offset={offset} size={size}");
    (offset, size)
}

/// Takes an offset and size created with value_to_host, writes them to memory, and returns an offset for the host to retrieve.
/// The length of the serialized offset and size are always known.
/// This is how we work around the limitation of a single return in Web Assembly. 
pub fn vec_parts_to_host(offset: u32, size: u32) -> u32 {
    let offset = offset.to_le_bytes();
    let len = size.to_le_bytes();

    let buffer: [u8; 8] = [
        offset[0], offset[1], offset[2], offset[3],
        len[0], len[1], len[2], len[3],
    ];
    let offset = Box::into_raw(Box::new(buffer));

    let out = offset as u32;
    println!("GUEST: vec_parts_to_host, out={out}");
    out
}

/// "Unforgets" a bit of memory we created for the host and drops it.
#[no_mangle]
pub fn unforget(offset: u32, size: u32) {
    println!("GUEST: unforget called, offset={offset}, size={size}");
    // We're happy this isn't used, we want to drop it.
    let _bytes = unsafe { Vec::from_raw_parts(offset as *mut u8, size as usize, size as usize) };
}

/// Converts a previously-stored vector present in our memory somewhere back into a real value for us to use.
/// Drops the original memory.
pub fn value_from_host<T>(offset: u32, size: u32) -> T where T: Sized + serde::de::DeserializeOwned {
    println!("GUEST: value_from_host, offset={offset}, size={size}");
    let vec = unsafe { Vec::from_raw_parts(offset as *mut u8, size as usize, size as usize) };
    let out: T = rmp_serde::decode::from_slice(&vec).expect("from_host<T>: error reading from memory");
    out
}

/// Reconstructs offset and size of a vec created with wasm_alloc.
pub fn vec_parts_from_host(offset: u32) -> (u32, u32) {
    let buf = unsafe { Box::<[u8; 8]>::from_raw(offset as *mut [u8; 8]) };
    let offset = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let size = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let out = (offset, size);
    println!("GUEST: vec_parts_from_host, out={out:?}");
    out
}

/// Prints to Middle console.
pub fn mprint<S: Into<String>>(input: S) {
    let input: String = input.into();
    let (offset, size) = value_to_host(&input);
    unsafe { host_print(offset, size) };
}

#[derive(Serialize)]
pub struct FnInfo {
    pub description: String,
    pub in_schema: RootSchema,
    pub out_schema: RootSchema,
}

// A resumable 
#[derive(Serialize, Deserialize)]
pub enum Resumable<T> {
    Pause,
    Ready(T)
}

impl<T> FromResidual for Resumable<T> {
    fn from_residual(residual: Resumable<convert::Infallible>) -> Self {
        match residual {
            Resumable::Pause => Resumable::Pause,
            // For some reason, the standard library doesn't have to match this branch. Why not? 
            // Maybe see... https://github.com/rust-lang/rust/issues/51085
            Resumable::Ready(_) => panic!("not reached"),
        }
    }
}

impl<T> Try for Resumable<T> {
    type Output = T;

    type Residual = Resumable<convert::Infallible>;

    fn from_output(output: Self::Output) -> Self {
        Resumable::Ready(output)
    }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self {
            Resumable::Pause => ControlFlow::Break(Resumable::Pause),
            Resumable::Ready(inner) => ControlFlow::Continue(inner),
        }
    }
}

/// Pause execution of this multi-step function.
pub fn pause(duration: Duration) -> Resumable<()> {
    let milis = duration.as_millis();
    let milis: u64 = milis.try_into().unwrap();
    let resume = unsafe { host_pause(milis) };
    match resume {
        0 => Resumable::Pause,
        _ => Resumable::Ready(()),
    } 
}

#[link(wasm_import_module = "middle")]
extern {
    pub fn host_print(offset: u32, size: u32);
    pub fn host_pause(millis: u64) -> u32;
    pub fn host_panic(offset: u32, size: u32);
}