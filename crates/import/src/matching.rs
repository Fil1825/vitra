//! How well a found track (in the library or on YouTube Music) fits an imported one: title,
//! artists and duration compared after [`normalize`](crate::normalize), versions that must
//! agree (a live take isn't the studio song), and ISRC when both sides have it.

use serde::{Deserialize, Serialize};

use crate::model::ImportedTrack;
use crate::normalize::{self, Title, fold, latin};

/// A track that might be the one: from the library or a search.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Candidate {
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    /// Seconds.
    pub duration: Option<f64>,
    pub isrc: Option<String>,
}

/// How sure a match is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Not the track.
    None,
    /// Perhaps: taken, but marked for the user to check.
    Doubtful,
    /// Taken without asking.
    Sure,
}

/// Taken without asking from here on.
const SURE: f64 = 0.82;
/// Offered as a possibility from here on.
const DOUBTFUL: f64 = 0.55;

impl Confidence {
    pub fn of(score: f64) -> Self {
        if score >= SURE {
            Self::Sure
        } else if score >= DOUBTFUL {
            Self::Doubtful
        } else {
            Self::None
        }
    }
}

/// How alike two folded texts are, 0..1: the better of their edit distance and their shared
/// words, in their own letters or both in Latin ones.
pub fn similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let direct = text_similarity(a, b);
    let (la, lb) = (latin(a), latin(b));
    if la == a && lb == b {
        return direct;
    }
    direct.max(text_similarity(&la, &lb))
}

fn text_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let chars = 1.0 - levenshtein(a, b) as f64 / a.chars().count().max(b.chars().count()) as f64;
    // Shared words: a missing subtitle or an extra word costs less than spelling does.
    let wa: Vec<&str> = a.split(' ').collect();
    let wb: Vec<&str> = b.split(' ').collect();
    let shared = wa.iter().filter(|w| wb.contains(w)).count() as f64;
    let words = if shared == 0.0 {
        0.0
    } else {
        let containment = shared / wa.len().min(wb.len()) as f64;
        let jaccard = shared / (wa.len() + wb.len()) as f64 * 2.0;
        // One title wholly inside the other ("Song" in "Song Part One") is close, not equal.
        (containment * 0.85).max(jaccard)
    };
    chars.max(words)
}

/// Edit distance in characters.
fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

/// How well `candidate` fits `wanted`, 0..1.
pub fn score(wanted: &ImportedTrack, candidate: &Candidate) -> f64 {
    let direct = score_one(wanted, candidate);
    match wanted.swapped() {
        Some(swapped) => direct.max(score_one(&swapped, candidate)),
        None => direct,
    }
}

fn score_one(wanted: &ImportedTrack, candidate: &Candidate) -> f64 {
    if let (Some(a), Some(b)) = (&wanted.isrc, &candidate.isrc)
        && !a.trim().is_empty()
        && a.trim().eq_ignore_ascii_case(b.trim())
    {
        return 1.0;
    }
    let ours = normalize::title(&wanted.title);
    let theirs = normalize::title(&candidate.title);
    let title = similarity(&ours.base, &theirs.base);
    let artists = artist_similarity(wanted, &ours, candidate, &theirs);
    let duration = match (wanted.duration, candidate.duration) {
        (Some(a), Some(b)) => {
            let off = (a - b).abs();
            if off <= 3.0 {
                1.0
            } else if off <= 7.0 {
                0.7
            } else if off <= 15.0 {
                0.3
            } else {
                0.0
            }
        }
        // Unknown on one side: neither for nor against.
        _ => 0.6,
    };
    let mismatched = ours.versions.symmetric_difference(&theirs.versions).count();
    let penalty = (mismatched as f64 * 0.25).min(0.5);
    // Nobody named on one side: a title and a length alone are rarely sure.
    let artists = artists.unwrap_or(0.4);
    // A different song by the same artist must not pass on artist and duration alone.
    let title_gate = if title < 0.6 { title } else { 1.0 };
    ((0.5 * title + 0.3 * artists + 0.2 * duration) * title_gate - penalty).clamp(0.0, 1.0)
}

/// The artists' fit: the main artist found among theirs (and those credited in their
/// title), `None` when one side names nobody.
fn artist_similarity(
    wanted: &ImportedTrack,
    ours: &Title,
    candidate: &Candidate,
    theirs: &Title,
) -> Option<f64> {
    let mut mine: Vec<String> = wanted
        .artists
        .iter()
        .flat_map(|a| normalize::artist_names(a))
        .collect();
    mine.extend(ours.featuring.iter().cloned());
    let mut others: Vec<String> = candidate
        .artists
        .iter()
        .flat_map(|a| normalize::artist_names(a))
        .collect();
    others.extend(theirs.featuring.iter().cloned());
    // A channel named "Artist - Topic" or "ArtistVEVO" names the artist.
    for other in &mut others {
        for suffix in [" topic", "vevo", " official"] {
            if let Some(stripped) = other.strip_suffix(suffix) {
                *other = stripped.trim().to_owned();
            }
        }
    }
    let main = mine.first()?;
    if others.is_empty() {
        return None;
    }
    let best = |name: &str| {
        others
            .iter()
            .map(|other| similarity(name, other))
            .fold(0.0, f64::max)
    };
    let main_fit = best(main);
    // The others credited count a little: their absence on one side is common.
    let rest = &mine[1..];
    if rest.is_empty() {
        return Some(main_fit);
    }
    let rest_fit = rest.iter().map(|name| best(name)).sum::<f64>() / rest.len() as f64;
    Some(main_fit * 0.85 + rest_fit * 0.15)
}

/// Candidates by how well they fit, best first, with their scores.
pub fn rank(wanted: &ImportedTrack, candidates: &[Candidate]) -> Vec<(usize, f64)> {
    let mut ranked: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (index, score(wanted, candidate)))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked
}

/// The words to search for a track with: its name without versions' noise, and its main
/// artist.
pub fn search_query(track: &ImportedTrack) -> String {
    let mut query = String::new();
    if let Some(artist) = track.artists.first() {
        query.push_str(artist.trim());
        query.push(' ');
    }
    // Keep what makes the version (a live take is searched as live); drop the rest.
    let title = normalize::title(&track.title);
    let mut name = title.base;
    for version in &title.versions {
        name.push(' ');
        name.push_str(match version {
            normalize::Version::Live => "live",
            normalize::Version::Remix => "remix",
            normalize::Version::Acoustic => "acoustic",
            normalize::Version::Instrumental => "instrumental",
            normalize::Version::Demo => "demo",
            normalize::Version::Karaoke => "karaoke",
            normalize::Version::SpedUp => "sped up",
            normalize::Version::Slowed => "slowed",
            normalize::Version::Cover => "cover",
        });
    }
    if name.trim().is_empty() {
        name = fold(&track.title);
    }
    query.push_str(&name);
    query.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted(title: &str, artist: &str, duration: Option<f64>) -> ImportedTrack {
        ImportedTrack {
            duration,
            ..ImportedTrack::new(title, vec![artist.into()])
        }
    }

    fn found(title: &str, artist: &str, duration: Option<f64>) -> Candidate {
        Candidate {
            title: title.into(),
            artists: vec![artist.into()],
            duration,
            ..Candidate::default()
        }
    }

    fn confidence(w: &ImportedTrack, c: &Candidate) -> Confidence {
        Confidence::of(score(w, c))
    }

    #[test]
    fn the_same_song_spelled_differently_is_sure() {
        let pairs = [
            (
                wanted(
                    "Here Comes the Sun - Remastered 2009",
                    "The Beatles",
                    Some(185.7),
                ),
                found(
                    "Here Comes The Sun (Remastered 2009)",
                    "The Beatles",
                    Some(186.0),
                ),
            ),
            (
                wanted("Bad Habits", "Ed Sheeran", Some(231.0)),
                found("Bad Habits", "Ed Sheeran - Topic", Some(232.0)),
            ),
            (
                wanted("Группа крови", "Кино", Some(285.0)),
                found("Gruppa krovi", "Kino", Some(286.0)),
            ),
            (
                wanted("Ёлка", "Ёлка", None),
                found("Елка", "Елка", Some(200.0)),
            ),
            (
                wanted(
                    "Peaches (feat. Daniel Caesar & Giveon)",
                    "Justin Bieber",
                    Some(198.0),
                ),
                found(
                    "Peaches",
                    "Justin Bieber, Daniel Caesar, Giveon",
                    Some(198.0),
                ),
            ),
            (
                wanted("Beyoncé", "Halo", None),
                found("Beyonce", "Halo", None),
            ),
        ];
        for (w, c) in &pairs {
            assert_eq!(
                confidence(w, c),
                Confidence::Sure,
                "{w:?} vs {c:?}: {}",
                score(w, c)
            );
        }
    }

    #[test]
    fn other_songs_and_versions_are_not() {
        // Another song of the same artist, same length.
        let w = wanted("Yesterday", "The Beatles", Some(125.0));
        assert_eq!(
            confidence(&w, &found("Let It Be", "The Beatles", Some(125.0))),
            Confidence::None
        );
        // The live take of the song.
        let studio = wanted("Bohemian Rhapsody", "Queen", Some(354.0));
        let live = found("Bohemian Rhapsody (Live Aid)", "Queen", Some(354.0));
        assert!(
            confidence(&studio, &live) < Confidence::Sure,
            "{}",
            score(&studio, &live)
        );
        // A cover by someone else.
        let cover = found("Bohemian Rhapsody", "Panic! At The Disco", Some(352.0));
        assert!(
            confidence(&studio, &cover) < Confidence::Sure,
            "{}",
            score(&studio, &cover)
        );
        // Far longer: probably a mix or another recording.
        let long = found("Bohemian Rhapsody", "Queen", Some(500.0));
        assert!(confidence(&studio, &long) < Confidence::Sure);
    }

    #[test]
    fn isrc_settles_it() {
        let mut w = wanted("Anything", "Anyone", None);
        w.isrc = Some("GBUM71029604".into());
        let mut c = found("Totally different", "Nobody", None);
        c.isrc = Some("gbum71029604".into());
        assert_eq!(score(&w, &c), 1.0);
    }

    #[test]
    fn either_side_of_a_dash_may_be_the_artist() {
        // "Title - Artist" read as "Artist - Title".
        let mut w = wanted("Daft Punk", "One More Time", None);
        w.swappable = true;
        let c = found("One More Time", "Daft Punk", Some(320.0));
        assert_eq!(confidence(&w, &c), Confidence::Sure);
    }

    #[test]
    fn ranks_best_first() {
        let w = wanted("Numb", "Linkin Park", Some(185.0));
        let candidates = [
            found("Numb/Encore", "JAY-Z, Linkin Park", Some(205.0)),
            found("Numb", "Linkin Park", Some(186.0)),
            found("In the End", "Linkin Park", Some(216.0)),
        ];
        let ranked = rank(&w, &candidates);
        assert_eq!(ranked[0].0, 1);
        assert_eq!(Confidence::of(ranked[0].1), Confidence::Sure);
    }

    #[test]
    fn queries_keep_versions() {
        assert_eq!(
            search_query(&wanted(
                "Hotel California - Live On MTV, 1994",
                "Eagles",
                None
            )),
            "Eagles hotel california live"
        );
        assert_eq!(
            search_query(&wanted(
                "Here Comes the Sun - Remastered 2009",
                "The Beatles",
                None
            )),
            "The Beatles here comes the sun"
        );
    }

    #[test]
    fn similarity_basics() {
        assert_eq!(similarity("abc", "abc"), 1.0);
        assert_eq!(similarity("", "abc"), 0.0);
        assert!(similarity("kino", "кино") > 0.99);
        assert!(similarity("the beatles", "beatles") > 0.6);
        assert!(similarity("yesterday", "let it be") < 0.3);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }
}
