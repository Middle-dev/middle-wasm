use std::time::Duration;

use serde::{Serialize, Deserialize};
use serde_json::Value;

use crate::{value_to_host, vec_parts_from_host, value_from_host};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct HostRequestResponse {
    // The status code of the response
    http_code: u32,

    // Raw headers on the response
    headers: Vec<(String, String)>,
    
    // Raw body of the response
    body: String,
}

impl HostRequestResponse {
    pub fn code(&self) -> u32 {
        self.http_code
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn json(&self) -> serde_json::Result<Value> {
        serde_json::from_str::<serde_json::Value>(&self.body)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct HostRequestOut (Result<HostRequestResponse, String>);

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RequestIn {
    url: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RequestOut {
    http_code: u32,
    body: String
}

/// Makes a request to an API with the given headers and payload.
/// Returns the status code and body.
pub fn request(input: &RequestBuilder) -> Result<HostRequestResponse, String> {
    let (offset, size) = value_to_host(input);
    let offset = unsafe { host_request(offset, size) };
    let (offset, size) = vec_parts_from_host(offset);
    let out: HostRequestOut = value_from_host(offset, size);
    out.0
}


#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum HostRequestType {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RequestBuilder {
    // URL to invoke.
    url: String,

    // Request method.
    method: HostRequestType,

    // Vector of (key, value) pairs to be included as HTTP headers.
    // Instead of setting an Authorization header yourself, consider using basic_auth or bearer_token.
    headers: Option<Vec<(String, String)>>,

    // Basic auth, in the form of (username, password).
    basic_auth: Option<(String, String)>, 
    // Bearer token. 

    bearer_auth: Option<String>,
    // Raw request body.
    // Try to use form or json instead.
    body: Option<String>,

    // Set a timeout for this request.
    // The timeout is applied from when the request starts connecting until the request body is finished.
    // This needs testing.
    // It's not clear how it will interact the asynchronous wasmtime runtime.
    timeout: Option<Duration>,

    // Send a form body. Also sets the Content-Type header to application/x-www-form-urlencoded.
    form: Option<Vec<(String, String)>>,

    // Send a JSON body.
    json: Option<Value>, 
}

impl RequestBuilder {
    pub fn new(url: String, method: HostRequestType) -> Self {
        Self {
            url,
            method,
            headers: None,
            basic_auth: None,
            bearer_auth: None,
            body: None,
            timeout: None,
            form: None,
            json: None
        }
    }
    pub fn get<S: Into<String>>(url: S) -> Self {
        Self::new(url.into(), HostRequestType::Get)
    }
    pub fn post<S: Into<String>>(url: S) -> Self {
        Self::new(url.into(), HostRequestType::Post)
    }
    pub fn with_json(mut self, value: Value) -> Self {
        self.json = Some(value);
        self
    }
    pub fn with_bearer_auth(mut self, bearer_token: String) -> Self {
        self.bearer_auth = Some(bearer_token);
        self
    }
    /// Sets a form parameter
    pub fn set_form_key<S: Into<String>, S1: Into<String>>(mut self, key: S, value: S1) -> Self {
        match &mut self.form {
            Some(form) => form.push((key.into(), value.into())),
            None => {
                self.form = Some(vec![(key.into(), value.into())]);
            },
        }
        self
    }
    /// Sets Basic Auth
    pub fn set_basic_auth<S: Into<String>, S1: Into<String>>(mut self, username: S, password: S1) -> Self {
        self.basic_auth = Some((username.into(), password.into()));
        self
    }
    /// Makes a request and returns a response.
    /// When invoked from the Middle runtime, keep in mind that this request will be run asynchronously. 
    pub fn call(&self) -> Result<HostRequestResponse, String> {
        request(self)
    }
}

#[link(wasm_import_module = "middle")]
extern {
    pub fn host_request(offset: u32, size: u32) -> u32;
}
