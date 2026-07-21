//! In-crate mock HTTP transport used by unit tests.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Not compiled into the published library; only used by `#[cfg(test)]` unit
//! tests so the protocol clients can be exercised without any real network.

#[cfg(test)]
pub(crate) struct MockTransport {
    responses: std::collections::HashMap<String, String>,
    pub commands: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    pub uploads: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>,
    pub posts: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>,
}

#[cfg(test)]
impl MockTransport {
    pub fn new() -> Self {
        MockTransport {
            responses: std::collections::HashMap::new(),
            commands: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            uploads: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            posts: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Register a canned response body for a request `path`.
    pub fn respond(mut self, path: &str, body: &str) -> Self {
        self.responses.insert(path.to_string(), body.to_string());
        self
    }
}

#[cfg(test)]
impl crate::transport::HttpTransport for MockTransport {
    fn get(&self, path: &str) -> Result<String, crate::PrinterError> {
        if path.starts_with("/?cmd=") {
            self.commands.lock().unwrap().push(path.to_string());
        }
        self.responses
            .get(path)
            .cloned()
            .ok_or_else(|| crate::PrinterError::NotFound(format!("no mock for {path}")))
    }

    fn post(&self, path: &str, body: &[u8], _ct: &str) -> Result<String, crate::PrinterError> {
        self.posts.lock().unwrap().push((path.to_string(), body.to_vec()));
        self.responses
            .get(path)
            .cloned()
            .ok_or_else(|| crate::PrinterError::NotFound(format!("no mock for {path}")))
    }

    fn upload(
        &self,
        path: &str,
        filename: &str,
        data: &[u8],
        _extra: &[(&str, &str)],
    ) -> Result<String, crate::PrinterError> {
        self.uploads.lock().unwrap().push((filename.to_string(), data.to_vec()));
        self.responses
            .get(path)
            .cloned()
            .ok_or_else(|| crate::PrinterError::NotFound(format!("no mock for {path}")))
    }
}
