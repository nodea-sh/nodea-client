//! Token bytes -> text, without ever emitting half a character.
//!
//! A token is a byte sequence, not a string: an emoji or an accented letter
//! can be split across two or three tokens. Decoding each token on its own
//! would emit U+FFFD for both halves.

#[derive(Default)]
pub(super) struct Utf8Assembler {
    pending: Vec<u8>,
}

impl Utf8Assembler {
    /// Append a token's bytes; returns every complete character so far.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    return out;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // SAFETY-free: `valid_up_to` bytes are valid UTF-8 by definition.
                    out.push_str(std::str::from_utf8(&self.pending[..valid]).unwrap());
                    match error.error_len() {
                        // An incomplete sequence at the end: wait for the next token.
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                        // Bytes that can never become valid: replace and move on.
                        Some(bad) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            self.pending.drain(..valid + bad);
                        }
                    }
                }
            }
        }
    }

    /// Whatever is still pending when generation ends, decoded lossily.
    pub fn finish(&mut self) -> String {
        let rest = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_passes_straight_through() {
        let mut utf8 = Utf8Assembler::default();
        assert_eq!(utf8.push(b"hello"), "hello");
        assert_eq!(utf8.finish(), "");
    }

    #[test]
    fn a_character_split_across_tokens_is_held_until_complete() {
        let mut utf8 = Utf8Assembler::default();
        let emoji = "🦀".as_bytes();

        assert_eq!(utf8.push(&emoji[..2]), "");
        assert_eq!(utf8.push(&emoji[2..]), "🦀");
    }

    #[test]
    fn complete_text_before_a_split_is_not_held_back() {
        let mut utf8 = Utf8Assembler::default();
        let e = "é".as_bytes();

        assert_eq!(utf8.push(&[b'a', e[0]]), "a");
        assert_eq!(utf8.push(&[e[1], b'b']), "éb");
    }

    #[test]
    fn invalid_bytes_become_one_replacement_each_and_do_not_stall() {
        let mut utf8 = Utf8Assembler::default();
        assert_eq!(utf8.push(&[b'a', 0xff, b'b']), "a\u{FFFD}b");
    }

    #[test]
    fn an_unfinished_sequence_at_the_end_is_flushed_lossily() {
        let mut utf8 = Utf8Assembler::default();
        assert_eq!(utf8.push(&"🦀".as_bytes()[..2]), "");
        assert_eq!(utf8.finish(), "\u{FFFD}");
    }
}
