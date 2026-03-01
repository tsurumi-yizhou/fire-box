//! Streaming helper for easier consumption of streaming responses.

use crate::{FireBoxClient, Result, StreamChunk};
use std::time::Duration;

/// A helper for reading streaming responses.
pub struct StreamReader<'a> {
    client: &'a FireBoxClient,
    stream_id: String,
    done: bool,
}

impl<'a> StreamReader<'a> {
    #[allow(dead_code)]
    pub(crate) fn new(client: &'a FireBoxClient, stream_id: String) -> Self {
        Self {
            client,
            stream_id,
            done: false,
        }
    }

    /// Get the stream ID.
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    /// Poll for the next batch of chunks.
    ///
    /// Returns `Ok(None)` when the stream is complete.
    pub fn poll(&mut self) -> Result<Option<Vec<StreamChunk>>> {
        if self.done {
            return Ok(None);
        }

        let chunks = self.client.stream_poll(&self.stream_id)?;

        if chunks.is_empty() {
            return Ok(Some(vec![]));
        }

        // Check if stream is done
        for chunk in &chunks {
            if matches!(chunk, StreamChunk::Done { .. } | StreamChunk::Error(_)) {
                self.done = true;
                break;
            }
        }

        Ok(Some(chunks))
    }

    /// Poll with a timeout, sleeping between polls.
    pub fn poll_blocking(&mut self, poll_interval: Duration) -> Result<Option<Vec<StreamChunk>>> {
        loop {
            let result = self.poll()?;

            if let Some(chunks) = result {
                if !chunks.is_empty() || self.done {
                    return Ok(Some(chunks));
                }
            } else {
                return Ok(None);
            }

            std::thread::sleep(poll_interval);
        }
    }

    /// Cancel the stream.
    pub fn cancel(&mut self) -> Result<()> {
        if !self.done {
            self.client.stream_cancel(&self.stream_id)?;
            self.done = true;
        }
        Ok(())
    }

    /// Check if the stream is done.
    pub fn is_done(&self) -> bool {
        self.done
    }
}

impl<'a> Drop for StreamReader<'a> {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.cancel();
        }
    }
}
