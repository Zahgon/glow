//! The HTTP client, with `net/http`'s observable behaviour.
//!
//! `http.Get` follows redirects and returns the response no matter its status,
//! leaving the status check to the caller; `ureq` treats a 4xx/5xx as an error,
//! so it is unwrapped back into a response here.

use std::io::Read;

/// A fetched HTTP response.
pub struct Response {
    /// The status code.
    pub status: u16,
    body: Vec<u8>,
}

impl Response {
    /// The body as text.
    pub fn into_string(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    /// The body as a reader.
    pub fn into_reader(self) -> Box<dyn Read> {
        Box::new(std::io::Cursor::new(self.body))
    }
}

/// Issues a GET request, returning the response for any status code.
pub fn get(url: &str) -> Result<Response, String> {
    let response = match ureq::get(url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => return Err(e.to_string()),
    };
    let status = response.status();
    let mut body = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    Ok(Response { status, body })
}
