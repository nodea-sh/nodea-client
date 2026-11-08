// Download speed for the UI, from the byte counts the download reports about
// once a second. A rolling window rather than the last interval alone: one
// slow or bursty chunk would otherwise make the figure jump every second.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const DEFAULT_WINDOW: Duration = Duration::from_secs(5);

pub struct RateMeter {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl RateMeter {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    /// Record the total downloaded so far and return bytes per second over
    /// the window. 0 until two samples span some time.
    pub fn record(&mut self, now: Instant, downloaded_bytes: u64) -> u64 {
        self.samples.push_back((now, downloaded_bytes));
        // Keep the newest sample at or beyond the window edge as the anchor,
        // so the window stays full rather than shrinking below `window`.
        while self.samples.len() > 2 && now.duration_since(self.samples[1].0) >= self.window {
            self.samples.pop_front();
        }

        let (first_at, first_bytes) = self.samples[0];
        let elapsed = now.duration_since(first_at).as_secs_f64();
        if elapsed <= 0.0 {
            return 0;
        }
        (downloaded_bytes.saturating_sub(first_bytes) as f64 / elapsed) as u64
    }
}

impl Default for RateMeter {
    fn default() -> Self {
        Self::new(DEFAULT_WINDOW)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1_000_000;

    #[test]
    fn no_rate_from_a_single_sample() {
        let mut meter = RateMeter::default();
        assert_eq!(meter.record(Instant::now(), 10 * MB), 0);
    }

    #[test]
    fn steady_download_reports_its_rate() {
        let start = Instant::now();
        let mut meter = RateMeter::default();
        let mut rate = 0;
        for s in 0..=10 {
            rate = meter.record(start + Duration::from_secs(s), s * 20 * MB);
        }
        assert_eq!(rate, 20 * MB);
    }

    #[test]
    fn rate_follows_a_slowdown_once_the_window_passes() {
        let start = Instant::now();
        let mut meter = RateMeter::default();
        let mut bytes = 0;
        for s in 0..=10 {
            bytes = s * 50 * MB;
            meter.record(start + Duration::from_secs(s), bytes);
        }
        let mut rate = 0;
        for s in 11..=20 {
            bytes += 10 * MB;
            rate = meter.record(start + Duration::from_secs(s), bytes);
        }
        assert_eq!(rate, 10 * MB);
    }

    #[test]
    fn one_slow_second_does_not_crater_the_rate() {
        let start = Instant::now();
        let mut meter = RateMeter::default();
        let mut bytes = 0;
        for s in 0..=5 {
            bytes = s * 20 * MB;
            meter.record(start + Duration::from_secs(s), bytes);
        }
        // One second with no data: the last interval alone would say 0.
        let rate = meter.record(start + Duration::from_secs(6), bytes);
        assert_eq!(rate, 16 * MB);
    }
}
