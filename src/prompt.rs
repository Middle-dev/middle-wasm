use anyhow::Error;
use schemars::{JsonSchema, schema::RootSchema};
use serde::{Serialize, Deserialize};
use serde_json::Value;

use crate::{Resumable, value_to_host, vec_parts_from_host, value_from_host};


/// Prompt the user to fill out a form.
/// The filled value, or an error, is returned.
pub fn prompt<'a, T>() -> Resumable<Result<T, Error>> where T: JsonSchema + for<'de> Deserialize<'de> {
    let schema = schemars::schema_for!(T);
    let value = match prompt_with_schema(schema)? {
        Ok(value) => value,
        Err(err) => return Resumable::Ready(Err(err)),
    };

    // Convert the value given by the host back into the type it's supposed to be in.
    let out: T = match serde_json::from_value(value) {
        Ok(out) => out,
        Err(err) => return Resumable::Ready(Err(Error::new(err).context("Deserialize error"))),
    };

    Resumable::Ready(Ok(out))
}

/// Prompt the user to fill out a form.
/// The form will prompt will be 
pub fn prompt_with_schema(schema: RootSchema) -> Resumable<Result<Value, Error>> {
    // Pass the schema to the host
    let prompt_info = PromptIn { schema };
    let (offset, size) = value_to_host(&prompt_info);

    // Call prompt
    let offset = unsafe { host_prompt(offset, size) };
    let (offset, size) = vec_parts_from_host(offset);
    let out: PromptOut = value_from_host(offset, size);

    // Escape if we need to pause. Escape if somehow there was an error.
    let value = match out.0? {
        Ok(value) => value,
        Err(err_str) => return Resumable::Ready(Err(Error::msg(err_str))),
    };

    // All done!
    Resumable::Ready(Ok(value))
}

#[derive(Serialize)]
struct PromptIn {
    schema: RootSchema,
}

#[derive(Deserialize)]
struct PromptOut (Resumable<Result<Value, String>>);

#[link(wasm_import_module = "middle")]
extern {
    pub fn host_prompt(offset: u32, size: u32) -> u32;
}
