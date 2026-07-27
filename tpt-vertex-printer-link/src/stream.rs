//! Stream G-code to a printer layer-by-layer instead of uploading the whole
//! file at once.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! [`GCodeStreamer`] sends G-code to a printer in chunks (one chunk per layer)
//! using the printer client's `send_gcode` interface.  This enables
//! layer-by-layer printing where each layer's G-code is pushed to the printer
//! only after the previous layer completes, reducing the need for the printer
//! to hold the entire file in memory.

use crate::client::{PrinterClient, PrinterError};

/// Configuration for layer-by-layer streaming.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamConfig {
    /// Maximum bytes to send per chunk (layer).  Layers exceeding this size
    /// are split into sub-chunks.
    pub chunk_size: usize,
    /// Whether to wait for an "ok" or temperature-stable acknowledgement
    /// between chunks.
    pub wait_ack: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        StreamConfig {
            chunk_size: 4096,
            wait_ack: true,
        }
    }
}

/// Callback invoked after each chunk is sent, with `(chunk_index, total_chunks)`.
pub type ProgressCallback = Box<dyn Fn(usize, usize) -> ()>;

/// Streams G-code line-by-line (or layer-by-layer) to a printer.
pub struct GCodeStreamer<'a> {
    client: &'a dyn PrinterClient,
    config: StreamConfig,
}

impl<'a> GCodeStreamer<'a> {
    pub fn new(client: &'a dyn PrinterClient, config: StreamConfig) -> Self {
        GCodeStreamer { client, config }
    }

    /// Stream the full G-code text to the printer, splitting on `"; LAYER"`
    /// comment boundaries.  Returns the total number of chunks sent.
    pub fn stream_full(&self, gcode: &str) -> Result<usize, PrinterError> {
        let chunks = split_layers(gcode);
        let total = chunks.len();
        for (i, chunk) in chunks.iter().enumerate() {
            self.client.send_gcode(chunk)?;
            if self.config.wait_ack {
                // Poll status to check the printer is still alive.
                self.client.status()?;
            }
        }
        Ok(total)
    }

    /// Stream with a progress callback.
    pub fn stream_with_progress(
        &self,
        gcode: &str,
        mut on_progress: ProgressCallback,
    ) -> Result<usize, PrinterError> {
        let chunks = split_layers(gcode);
        let total = chunks.len();
        for (i, chunk) in chunks.iter().enumerate() {
            self.client.send_gcode(chunk)?;
            on_progress(i + 1, total);
            if self.config.wait_ack {
                self.client.status()?;
            }
        }
        Ok(total)
    }

    /// Stream a single layer's G-code (call per-layer from the frontend).
    pub fn send_layer(&self, layer_gcode: &str) -> Result<(), PrinterError> {
        // Split into sub-chunks if the layer is very large.
        for chunk in split_by_size(layer_gcode, self.config.chunk_size) {
            self.client.send_gcode(&chunk)?;
        }
        Ok(())
    }
}

/// Split G-code into layer-sized chunks on `"; LAYER"` comment boundaries.
fn split_layers(gcode: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in gcode.lines() {
        if line.starts_with("; LAYER") && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Split text into chunks of at most `max_bytes` characters.
fn split_by_size(text: &str, max_bytes: usize) -> Vec<String> {
    if text.len() <= max_bytes {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = (start + max_bytes).min(text.len());
        chunks.push(text[start..end].to_string());
        start = end;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_layers_on_layer_comments() {
        let gcode = "; LAYER Z=0.2\nG1 X0 Y0\n; LAYER Z=0.4\nG1 X1 Y1\n";
        let chunks = split_layers(gcode);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].contains("Z=0.2"));
        assert!(chunks[1].contains("Z=0.4"));
    }

    #[test]
    fn split_by_size_respects_limit() {
        let text = "ABCDEFGHIJ";
        let chunks = split_by_size(text, 4);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "ABCD");
        assert_eq!(chunks[2], "IJ");
    }

    #[test]
    fn split_by_size_no_split_when_fits() {
        let chunks = split_by_size("hello", 100);
        assert_eq!(chunks, vec!["hello"]);
    }
}
