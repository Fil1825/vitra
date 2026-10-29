//! A persistent stream decoder problem, rather than a strike against every song.

use serde::Serialize;

/// Preserve TLS failures for diagnosis. Only explicit HTTP status messages mean throttling;
/// signed stream URLs may contain arbitrary digit sequences such as "403".
pub fn outage(message: &str) -> Option<links::Outage> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("certificate") || lower.contains("tls setup") {
        return None;
    }
    if [
        "http error 403",
        "http error 429",
        "server returned 403",
        "server returned 429",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
    {
        return Some(links::Outage::RateLimited);
    }
    links::ytdlp::outage(message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamProblem {
    Certificates,
    Decoder,
}

#[derive(Default)]
pub struct StreamFailures {
    last: Option<StreamProblem>,
    streak: u32,
}

impl StreamFailures {
    pub fn failed(&mut self, message: &str) {
        let message = message.to_ascii_lowercase();
        let problem = if message.contains("certificate") || message.contains("tls setup") {
            StreamProblem::Certificates
        } else {
            StreamProblem::Decoder
        };
        self.streak = if self.last == Some(problem) {
            self.streak.saturating_add(1)
        } else {
            1
        };
        self.last = Some(problem);
    }

    pub fn recovered(&mut self) {
        *self = Self::default();
    }

    pub fn problem(&self) -> Option<StreamProblem> {
        self.last.filter(|_| self.streak >= 5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_failures_and_url_digits_are_not_rate_limits() {
        assert_eq!(
            outage("FFmpeg failed: https://media.example/signed403: certificate verify failed"),
            None
        );
        assert_eq!(
            outage("FFmpeg failed: https://media.example/signed429: Input/output error"),
            None
        );
        assert_eq!(
            outage("[https] HTTP error 403 Forbidden"),
            Some(links::Outage::RateLimited)
        );
        assert_eq!(
            outage("[tcp] Connection refused"),
            Some(links::Outage::Unreachable)
        );
    }

    #[test]
    fn repeated_tls_failures_are_visible_until_a_successful_decode() {
        let mut failures = StreamFailures::default();
        for _ in 0..4 {
            failures.failed("FFmpeg failed: certificate verify failed");
        }
        assert_eq!(failures.problem(), None);
        failures.failed("FFmpeg failed: certificate verify failed on a different stream");
        assert_eq!(failures.problem(), Some(StreamProblem::Certificates));
        failures.recovered();
        assert_eq!(failures.problem(), None);
    }

    #[test]
    fn different_causes_restart_the_streak() {
        let mut failures = StreamFailures::default();
        for _ in 0..5 {
            failures.failed("FFmpeg failed: invalid data");
        }
        assert_eq!(failures.problem(), Some(StreamProblem::Decoder));
        failures.failed("FFmpeg failed: TLS setup: no certificate store");
        assert_eq!(failures.problem(), None);
    }
}
