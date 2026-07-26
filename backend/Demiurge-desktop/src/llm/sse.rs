//! Minimal Server-Sent Events decoder used by the streaming provider adapters.
//!
//! `reqwest` chunks are transport-level fragments: they are not guaranteed to
//! line up with UTF-8 characters, SSE lines, or complete events. This decoder
//! therefore buffers bytes until a complete line is available and only emits
//! an event after its blank-line delimiter (or when [`SseDecoder::finish`] is
//! called at EOF).

use std::fmt;

const MAX_SSE_LINE_BYTES: usize = 256 * 1024;
const MAX_SSE_EVENT_BYTES: usize = 1024 * 1024;
const MAX_SSE_PENDING_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Clone, Copy, Debug)]
struct SseLimits {
    line_bytes: usize,
    event_bytes: usize,
    pending_bytes: usize,
}

impl Default for SseLimits {
    fn default() -> Self {
        Self {
            line_bytes: MAX_SSE_LINE_BYTES,
            event_bytes: MAX_SSE_EVENT_BYTES,
            pending_bytes: MAX_SSE_PENDING_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SseDecodeError {
    LineTooLong { actual: usize, limit: usize },
    EventTooLarge { actual: usize, limit: usize },
    PendingBufferTooLarge { actual: usize, limit: usize },
}

impl SseDecodeError {
    #[cfg(test)]
    fn kind(&self) -> &'static str {
        match self {
            Self::LineTooLong { .. } => "line",
            Self::EventTooLarge { .. } => "event",
            Self::PendingBufferTooLarge { .. } => "buffer",
        }
    }
}

impl fmt::Display for SseDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineTooLong { actual, limit } => write!(
                formatter,
                "SSE line is {actual} bytes, exceeding the {limit} bytes limit"
            ),
            Self::EventTooLarge { actual, limit } => write!(
                formatter,
                "SSE event data is {actual} bytes, exceeding the {limit} bytes limit"
            ),
            Self::PendingBufferTooLarge { actual, limit } => write!(
                formatter,
                "SSE pending buffer is {actual} bytes, exceeding the {limit} bytes limit"
            ),
        }
    }
}

impl std::error::Error for SseDecodeError {}

#[derive(Debug)]
pub(super) struct SseDecoder {
    buffer: Vec<u8>,
    event: Option<String>,
    data_lines: Vec<String>,
    event_data_bytes: usize,
    at_stream_start: bool,
    limits: SseLimits,
}

impl Default for SseDecoder {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            event: None,
            data_lines: Vec::new(),
            event_data_bytes: 0,
            at_stream_start: true,
            limits: SseLimits::default(),
        }
    }
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn with_limits(line_bytes: usize, event_bytes: usize, pending_bytes: usize) -> Self {
        Self {
            limits: SseLimits {
                line_bytes,
                event_bytes,
                pending_bytes,
            },
            ..Self::default()
        }
    }

    /// Adds an arbitrary transport chunk and returns all complete SSE events.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, SseDecodeError> {
        let pending = self.pending_bytes().saturating_add(chunk.len());
        if pending > self.limits.pending_bytes {
            return Err(SseDecodeError::PendingBufferTooLarge {
                actual: pending,
                limit: self.limits.pending_bytes,
            });
        }
        self.buffer.extend_from_slice(chunk);
        self.drain_complete_lines()
    }

    /// Flushes an event whose final line or blank-line delimiter was omitted.
    pub fn finish(&mut self) -> Result<Vec<SseEvent>, SseDecodeError> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let mut tail = std::mem::take(&mut self.buffer);
            if tail.last() == Some(&b'\r') {
                tail.pop();
            }
            self.ensure_line_limit(tail.len())?;
            let line = String::from_utf8_lossy(&tail).into_owned();
            if let Some(event) = self.process_line(&line)? {
                events.push(event);
            }
        }

        if let Some(event) = self.dispatch_event() {
            events.push(event);
        }
        Ok(events)
    }

    fn drain_complete_lines(&mut self) -> Result<Vec<SseEvent>, SseDecodeError> {
        let mut events = Vec::new();
        while let Some((line_len, ending_len)) = next_line(&self.buffer) {
            self.ensure_line_limit(line_len)?;
            let consumed = line_len + ending_len;
            let line = String::from_utf8_lossy(&self.buffer[..line_len]).into_owned();
            self.buffer.drain(..consumed);
            if let Some(event) = self.process_line(&line)? {
                events.push(event);
            }
        }
        self.ensure_line_limit(self.buffer.len())?;
        self.ensure_pending_limit()?;
        Ok(events)
    }

    fn process_line(&mut self, line: &str) -> Result<Option<SseEvent>, SseDecodeError> {
        let line = if self.at_stream_start {
            self.at_stream_start = false;
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };

        if line.is_empty() {
            return Ok(self.dispatch_event());
        }
        if line.starts_with(':') {
            return Ok(None);
        }

        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => {
                let separator_bytes = usize::from(!self.data_lines.is_empty());
                let actual = self
                    .event_data_bytes
                    .saturating_add(separator_bytes)
                    .saturating_add(value.len());
                if actual > self.limits.event_bytes {
                    return Err(SseDecodeError::EventTooLarge {
                        actual,
                        limit: self.limits.event_bytes,
                    });
                }
                self.event_data_bytes = actual;
                self.data_lines.push(value.to_string());
            }
            // `id`, `retry`, and extension fields do not affect payload
            // decoding in these stateless HTTP streams.
            _ => {}
        }
        self.ensure_pending_limit()?;
        Ok(None)
    }

    fn dispatch_event(&mut self) -> Option<SseEvent> {
        let event = self.event.take();
        if self.data_lines.is_empty() {
            self.event_data_bytes = 0;
            return None;
        }
        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        self.event_data_bytes = 0;
        Some(SseEvent { event, data })
    }

    fn pending_bytes(&self) -> usize {
        self.buffer
            .len()
            .saturating_add(self.event_data_bytes)
            .saturating_add(self.event.as_ref().map_or(0, String::len))
    }

    fn ensure_line_limit(&self, actual: usize) -> Result<(), SseDecodeError> {
        if actual > self.limits.line_bytes {
            return Err(SseDecodeError::LineTooLong {
                actual,
                limit: self.limits.line_bytes,
            });
        }
        Ok(())
    }

    fn ensure_pending_limit(&self) -> Result<(), SseDecodeError> {
        let actual = self.pending_bytes();
        if actual > self.limits.pending_bytes {
            return Err(SseDecodeError::PendingBufferTooLarge {
                actual,
                limit: self.limits.pending_bytes,
            });
        }
        Ok(())
    }
}

/// Returns the content and terminator lengths of the next complete SSE line.
/// A trailing CR is held until the next chunk so a split CRLF is consumed as a
/// single line ending.
fn next_line(bytes: &[u8]) -> Option<(usize, usize)> {
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'\n' => return Some((index, 1)),
            b'\r' if index + 1 < bytes.len() => {
                let ending_len = if bytes[index + 1] == b'\n' { 2 } else { 1 };
                return Some((index, ending_len));
            }
            b'\r' => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_byte_fragmented_crlf_comments_and_multiline_data() {
        let input = b": heartbeat\r\nevent: message\r\ndata: {\"answer\":\r\ndata: 42}\r\n\r\n";
        let mut decoder = SseDecoder::new();
        let mut events = Vec::new();

        for byte in input {
            events.extend(decoder.push(std::slice::from_ref(byte)).unwrap());
        }
        events.extend(decoder.finish().unwrap());

        assert_eq!(
            events,
            vec![SseEvent {
                event: Some("message".to_string()),
                data: "{\"answer\":\n42}".to_string(),
            }]
        );
    }

    #[test]
    fn flushes_final_event_without_a_line_ending() {
        let mut decoder = SseDecoder::new();
        assert!(decoder
            .push(b"event: tail\ndata: final")
            .unwrap()
            .is_empty());

        assert_eq!(
            decoder.finish().unwrap(),
            vec![SseEvent {
                event: Some("tail".to_string()),
                data: "final".to_string(),
            }]
        );
    }

    #[test]
    fn accepts_cr_line_endings_and_ignores_fields_without_data() {
        let mut decoder = SseDecoder::new();
        let mut events = decoder
            .push(b"event: ignored\r\rdata: one\r\rdata: two\r\r")
            .unwrap();
        events.extend(decoder.finish().unwrap());

        assert_eq!(
            events,
            vec![
                SseEvent {
                    event: None,
                    data: "one".to_string(),
                },
                SseEvent {
                    event: None,
                    data: "two".to_string(),
                },
            ]
        );
    }

    #[test]
    fn strips_only_one_optional_space_after_data_field() {
        let mut decoder = SseDecoder::new();
        let events = decoder
            .push(b"data:  leading\n\ndata:no-leading\n\n")
            .unwrap();

        assert_eq!(events[0].data, " leading");
        assert_eq!(events[1].data, "no-leading");
    }

    #[test]
    fn rejects_a_line_that_exceeds_the_configured_limit() {
        let mut decoder = SseDecoder::with_limits(8, 64, 128);

        let error = decoder.push(b"data: 1234").unwrap_err();

        assert_eq!(error.kind(), "line");
        assert!(error.to_string().contains("8 bytes"));
    }

    #[test]
    fn rejects_multiline_event_data_that_exceeds_the_configured_limit() {
        let mut decoder = SseDecoder::with_limits(64, 5, 128);

        let error = decoder.push(b"data: abc\ndata: def\n\n").unwrap_err();

        assert_eq!(error.kind(), "event");
        assert!(error.to_string().contains("5 bytes"));
    }

    #[test]
    fn rejects_an_incoming_chunk_that_exceeds_the_pending_buffer_limit() {
        let mut decoder = SseDecoder::with_limits(64, 64, 8);

        let error = decoder.push(b": 123456789").unwrap_err();

        assert_eq!(error.kind(), "buffer");
        assert!(error.to_string().contains("8 bytes"));
    }
}
