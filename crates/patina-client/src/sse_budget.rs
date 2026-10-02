//! Bound incomplete SSE frames before eventsource-stream allocates a complete event.
//! Framing/UTF-8 interpretation remains owned by eventsource-stream.
const MAX_FRAME_BYTES: usize = 128 * 1024;

#[derive(Default)]
pub(crate) struct FrameBudget {
    frame_bytes: usize,
    line_bytes: usize,
    previous_cr: bool,
    failed: bool,
}

impl FrameBudget {
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        if self.failed {
            return Err("patinad SSE frame exceeded its size limit");
        }
        for &byte in bytes {
            // A split CRLF is a single line ending. After a blank CR line,
            // its LF belongs to the completed frame, not the following one.
            if self.previous_cr && byte == b'\n' {
                self.previous_cr = false;
                if self.frame_bytes != 0 {
                    self.frame_bytes += 1;
                }
            } else {
                self.previous_cr = byte == b'\r';
                self.frame_bytes += 1;
                if self.frame_bytes > MAX_FRAME_BYTES {
                    self.failed = true;
                    return Err("patinad SSE frame exceeded its size limit");
                }
                if matches!(byte, b'\r' | b'\n') {
                    if self.line_bytes == 0 {
                        self.frame_bytes = 0;
                    }
                    self.line_bytes = 0;
                } else {
                    self.line_bytes += 1;
                }
            }
            if self.frame_bytes > MAX_FRAME_BYTES {
                self.failed = true;
                return Err("patinad SSE frame exceeded its size limit");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmented_unterminated_frames_are_bounded_and_failure_is_terminal() {
        let mut budget = FrameBudget::default();
        budget.feed(b"data: ").unwrap();
        for _ in 0..127 {
            budget.feed(&[b'x'; 1024]).unwrap();
        }
        assert!(budget.feed(&[b'x'; 1024]).is_err());
        assert!(budget.feed(b"\n\ndata: valid\n\n").is_err());
    }

    #[test]
    fn completed_events_and_keepalives_do_not_accumulate_budget() {
        for separator in ["\n", "\r\n", "\r"] {
            let input =
                format!("data: test{separator}{separator}: heartbeat{separator}{separator}");
            let mut budget = FrameBudget::default();
            for _ in 0..10_000 {
                for byte in input.bytes() {
                    budget.feed(&[byte]).unwrap();
                }
            }
        }
    }

    #[test]
    fn many_data_lines_in_one_event_share_one_budget() {
        let mut budget = FrameBudget::default();
        let line = format!("data: {}\n", "x".repeat(1024));
        let result = (0..200).try_for_each(|_| budget.feed(line.as_bytes()));
        assert!(result.is_err());
    }
}
