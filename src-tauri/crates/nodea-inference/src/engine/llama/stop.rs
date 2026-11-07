//! OpenAI `stop` sequences over streamed text.
//!
//! A stop string can arrive split across tokens ("," then " 4"), so text that
//! could be the start of one is held back until it either completes the stop
//! (and is dropped) or cannot (and is released). The matched stop text itself
//! is never emitted.

pub(super) struct StopMatcher {
    stops: Vec<String>,
    held: String,
}

pub(super) struct Pushed {
    /// Text that can safely go to the client now.
    pub emit: String,
    /// A stop sequence matched; generation should end.
    pub stopped: bool,
}

impl StopMatcher {
    pub fn new(stops: &[String]) -> Self {
        Self {
            stops: stops.iter().filter(|s| !s.is_empty()).cloned().collect(),
            held: String::new(),
        }
    }

    pub fn push(&mut self, text: &str) -> Pushed {
        self.held.push_str(text);

        if let Some(at) = self
            .stops
            .iter()
            .filter_map(|stop| self.held.find(stop.as_str()))
            .min()
        {
            let emit = self.held[..at].to_string();
            self.held.clear();
            return Pushed {
                emit,
                stopped: true,
            };
        }

        let keep = self.partial_suffix_len();
        let split = self.held.len() - keep;
        let emit = self.held[..split].to_string();
        self.held.drain(..split);
        Pushed {
            emit,
            stopped: false,
        }
    }

    /// The held text, once nothing more is coming.
    pub fn finish(&mut self) -> String {
        std::mem::take(&mut self.held)
    }

    /// The longest suffix of `held` that is a proper prefix of some stop.
    fn partial_suffix_len(&self) -> usize {
        let mut longest = 0;
        for stop in &self.stops {
            for (end, _) in stop.char_indices().skip(1) {
                let prefix = &stop[..end];
                if end > longest && self.held.ends_with(prefix) {
                    longest = end;
                }
            }
        }
        longest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher(stops: &[&str]) -> StopMatcher {
        StopMatcher::new(&stops.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn no_stops_releases_everything() {
        let mut m = matcher(&[]);
        let pushed = m.push("hello");
        assert_eq!(pushed.emit, "hello");
        assert!(!pushed.stopped);
    }

    #[test]
    fn a_stop_in_one_piece_is_dropped_with_what_follows() {
        let mut m = matcher(&[", 4"]);
        let pushed = m.push("1, 2, 3, 4, 5");
        assert_eq!(pushed.emit, "1, 2, 3");
        assert!(pushed.stopped);
    }

    #[test]
    fn a_stop_split_across_tokens_is_held_then_dropped() {
        let mut m = matcher(&[", 4"]);
        assert_eq!(m.push("1, 2, 3").emit, "1, 2, 3");
        let held = m.push(",");
        assert_eq!(held.emit, "");
        assert!(!held.stopped);
        let done = m.push(" 4");
        assert_eq!(done.emit, "");
        assert!(done.stopped);
    }

    #[test]
    fn held_text_that_cannot_become_a_stop_is_released() {
        let mut m = matcher(&["END"]);
        assert_eq!(m.push("the E").emit, "the ");
        assert_eq!(m.push("ND? no, EX").emit, "");
    }

    #[test]
    fn held_text_is_released_when_it_diverges() {
        let mut m = matcher(&["END"]);
        assert_eq!(m.push("E").emit, "");
        assert_eq!(m.push("X").emit, "EX");
    }

    #[test]
    fn the_earliest_of_several_stops_wins() {
        let mut m = matcher(&["b", "a"]);
        let pushed = m.push("xxaxb");
        assert_eq!(pushed.emit, "xx");
        assert!(pushed.stopped);
    }

    #[test]
    fn finish_flushes_a_partial_match() {
        let mut m = matcher(&["END"]);
        assert_eq!(m.push("EN").emit, "");
        assert_eq!(m.finish(), "EN");
    }

    #[test]
    fn multibyte_stops_split_on_char_boundaries() {
        let mut m = matcher(&["é!"]);
        assert_eq!(m.push("café").emit, "caf");
        assert!(m.push("!").stopped);
    }
}
