//! Minimal Server-Sent Events decoder used by the streaming provider adapters.
//!
//! `reqwest` chunks are transport-level fragments: they are not guaranteed to
//! line up with UTF-8 characters, SSE lines, or complete events. This decoder
//! therefore buffers bytes until a complete line is available and only emits
//! an event after its blank-line delimiter (or when [`SseDecoder::finish`] is
//! called at EOF).

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug)]
pub(super) struct SseDecoder {
    buffer: Vec<u8>,
    event: Option<String>,
    data_lines: Vec<String>,
    at_stream_start: bool,
}

impl Default for SseDecoder {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            event: None,
            data_lines: Vec::new(),
            at_stream_start: true,
        }
    }
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an arbitrary transport chunk and returns all complete SSE events.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(chunk);
        self.drain_complete_lines()
    }

    /// Flushes an event whose final line or blank-line delimiter was omitted.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut events = if self.buffer.is_empty() {
            Vec::new()
        } else {
            // A synthetic LF makes the final unterminated line consumable. It
            // also completes a CR that arrived as the last byte of the stream.
            self.buffer.push(b'\n');
            self.drain_complete_lines()
        };

        if let Some(event) = self.dispatch_event() {
            events.push(event);
        }
        events
    }

    fn drain_complete_lines(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        while let Some((line_len, ending_len)) = next_line(&self.buffer) {
            let consumed = line_len + ending_len;
            let line = String::from_utf8_lossy(&self.buffer[..line_len]).into_owned();
            self.buffer.drain(..consumed);
            if let Some(event) = self.process_line(&line) {
                events.push(event);
            }
        }
        events
    }

    fn process_line(&mut self, line: &str) -> Option<SseEvent> {
        let line = if self.at_stream_start {
            self.at_stream_start = false;
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };

        if line.is_empty() {
            return self.dispatch_event();
        }
        if line.starts_with(':') {
            return None;
        }

        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data_lines.push(value.to_string()),
            // `id`, `retry`, and extension fields do not affect payload
            // decoding in these stateless HTTP streams.
            _ => {}
        }
        None
    }

    fn dispatch_event(&mut self) -> Option<SseEvent> {
        let event = self.event.take();
        if self.data_lines.is_empty() {
            return None;
        }
        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        Some(SseEvent { event, data })
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
            events.extend(decoder.push(std::slice::from_ref(byte)));
        }
        events.extend(decoder.finish());

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
        assert!(decoder.push(b"event: tail\ndata: final").is_empty());

        assert_eq!(
            decoder.finish(),
            vec![SseEvent {
                event: Some("tail".to_string()),
                data: "final".to_string(),
            }]
        );
    }

    #[test]
    fn accepts_cr_line_endings_and_ignores_fields_without_data() {
        let mut decoder = SseDecoder::new();
        let mut events = decoder.push(b"event: ignored\r\rdata: one\r\rdata: two\r\r");
        events.extend(decoder.finish());

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
        let events = decoder.push(b"data:  leading\n\ndata:no-leading\n\n");

        assert_eq!(events[0].data, " leading");
        assert_eq!(events[1].data, "no-leading");
    }
}
