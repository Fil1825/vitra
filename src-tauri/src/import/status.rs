//! What the background matcher is doing, for the playlist pages, and how a failed search
//! is taken: a wait, a stop until the settings change, or one strike against the entry.

use std::time::Duration;

use links::{LinkError, Outage};
use serde::Serialize;

use crate::error::AppError;

/// Sent whenever the matcher's state changes.
pub const STATUS_EVENT: &str = "import://status";

/// A run of outages waits 30 s, then twice as long each time, at most this.
const LONGEST_PAUSE: Duration = Duration::from_secs(10 * 60);
const FIRST_PAUSE: Duration = Duration::from_secs(30);

/// Failed searches for one entry before it is marked "not found" and the matcher moves on.
pub const ENTRY_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum MatcherStatus {
    /// Nothing waits to be looked for.
    Idle,
    /// Looking for an entry of this playlist.
    #[serde(rename_all = "camelCase")]
    Searching { playlist_id: i64, title: String },
    /// Waiting out an outage; asks again at `retry_at` (Unix ms), or sooner when told to.
    #[serde(rename_all = "camelCase")]
    Paused { reason: PauseReason, retry_at: u64 },
    /// Can't go on until something changes in the settings (or the user retries).
    #[serde(rename_all = "camelCase")]
    Blocked { reason: Blocker, detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PauseReason {
    /// YouTube asked to slow down.
    RateLimited,
    /// No connection, or the site doesn't answer.
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Blocker {
    /// The browser chosen for cookies can't be read (macOS keeps Safari's from apps without
    /// Full Disk Access).
    Cookies,
    /// No yt-dlp to search with.
    YtDlpMissing,
    /// YouTube wants a signed-in user and no browser is chosen for cookies.
    SignIn,
}

/// What a failed search means for the matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trouble {
    /// Nothing will work until the settings change.
    Blocked(Blocker),
    /// Everything fails for now; wait and try again.
    Paused(PauseReason),
    /// This entry's search failed; another may well work.
    Entry,
}

pub fn trouble(err: &AppError) -> Trouble {
    match err {
        AppError::Link(LinkError::YtDlpMissing) => Trouble::Blocked(Blocker::YtDlpMissing),
        AppError::Link(LinkError::Cookies(_)) => Trouble::Blocked(Blocker::Cookies),
        AppError::Link(LinkError::SignInRequired) => Trouble::Blocked(Blocker::SignIn),
        AppError::Link(link) => match link.outage() {
            Some(Outage::RateLimited) => Trouble::Paused(PauseReason::RateLimited),
            Some(Outage::Unreachable) => Trouble::Paused(PauseReason::Offline),
            None => Trouble::Entry,
        },
        // curl's own failures (`http.rs`): the network, not the page.
        AppError::Import(message)
            if message.starts_with("cannot reach the site")
                || message.starts_with("the site did not answer")
                || message.starts_with("cannot run curl") =>
        {
            Trouble::Paused(PauseReason::Offline)
        }
        _ => Trouble::Entry,
    }
}

/// How long the `streak`-th outage in a row waits (1-based).
pub fn pause(streak: u32) -> Duration {
    let doublings = streak.saturating_sub(1).min(8);
    (FIRST_PAUSE * 2u32.pow(doublings)).min(LONGEST_PAUSE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_fail_in_three_ways() {
        let cookies = AppError::Link(LinkError::Cookies(
            "[Errno 1] Operation not permitted: '/Users/me/Library/Containers/com.apple.Safari/\
             Data/Library/Cookies/Cookies.binarycookies'"
                .into(),
        ));
        assert_eq!(trouble(&cookies), Trouble::Blocked(Blocker::Cookies));
        assert_eq!(
            trouble(&AppError::Link(LinkError::YtDlpMissing)),
            Trouble::Blocked(Blocker::YtDlpMissing)
        );
        assert_eq!(
            trouble(&AppError::Link(LinkError::Failed(
                "Unable to download API page: HTTP Error 429: Too Many Requests".into()
            ))),
            Trouble::Paused(PauseReason::RateLimited)
        );
        assert_eq!(
            trouble(&AppError::Link(LinkError::TimedOut)),
            Trouble::Paused(PauseReason::Offline)
        );
        assert_eq!(
            trouble(&AppError::Import(
                "cannot reach the site: curl: (6) Could not resolve host".into()
            )),
            Trouble::Paused(PauseReason::Offline)
        );
        assert_eq!(
            trouble(&AppError::Link(LinkError::Failed(
                "[youtube] abc: Video unavailable".into()
            ))),
            Trouble::Entry
        );
    }

    #[test]
    fn pauses_grow_to_ten_minutes() {
        assert_eq!(pause(1), Duration::from_secs(30));
        assert_eq!(pause(2), Duration::from_secs(60));
        assert_eq!(pause(4), Duration::from_secs(240));
        assert_eq!(pause(6), LONGEST_PAUSE);
        assert_eq!(pause(60), LONGEST_PAUSE);
    }

    #[test]
    fn statuses_say_what_the_pages_need() {
        let json = serde_json::to_value(MatcherStatus::Paused {
            reason: PauseReason::RateLimited,
            retry_at: 5,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"state": "paused", "reason": "rateLimited", "retryAt": 5})
        );
        let json = serde_json::to_value(MatcherStatus::Searching {
            playlist_id: 3,
            title: "тайны".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"state": "searching", "playlistId": 3, "title": "тайны"})
        );
    }
}
