//! The wave: an endless queue that picks the next track by mood. Every analyzed track is a
//! point in a small mood space (energy, tempo, brightness, major/minor, how much the loudness
//! moves), each axis ranked against the library itself, so "energetic" means energetic for
//! this library. The wave steers a **course** through that space: each pick is a weighted
//! random choice among the tracks closest to the course (plus what the listener's habits
//! say), and the course drifts towards what is listened to, a little at random, held near a
//! liked track and pushed away from a disliked one.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::features::Features;

/// Energy, tempo, brightness, valence, spread.
pub const DIMS: usize = 5;
pub type Point = [f32; DIMS];

/// How much each axis matters to "sounds alike".
const WEIGHTS: Point = [1.0, 0.8, 0.6, 0.7, 0.3];
/// How far from the course a track still feels close (a Gaussian's width).
const CLOSENESS_WIDTH: f32 = 0.2;
/// The best this many are the choice; a softmax over their scores picks one.
const SHORTLIST: usize = 12;
const TEMPERATURE: f32 = 0.15;
/// How far the course may wander from where the mood put it.
const MAX_DRIFT: f32 = 0.35;
/// A random step per pick, so the wave doesn't settle in one corner.
const WALK: f32 = 0.03;
/// Tracks played this recently aren't picked (when there is anything else).
const RECENT_SECONDS: i64 = 2 * 3600;
/// ...and for a day after, count against.
const STALE_SECONDS: i64 = 24 * 3600;
/// A liked track holds the course near it for this many picks.
const HOLD_PICKS: u32 = 4;

/// What the wave knows about a track.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: i64,
    pub features: Features,
    /// Artist ids, the main one first.
    pub artists: Vec<i64>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub favorite: bool,
    pub plays: u32,
    /// Unix seconds.
    pub last_played: Option<i64>,
}

/// The listener's habits outside this wave.
#[derive(Debug, Clone, Default)]
pub struct Habits {
    /// Skips per track, already faded with time (a skip a month ago counts less): 0 = none.
    pub skips: HashMap<i64, f32>,
    /// Tracks marked "don't like": never picked.
    pub dislikes: HashSet<i64>,
    /// How often one track was played right after another.
    pub transitions: HashMap<(i64, i64), u32>,
}

/// Where a wave starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "track")]
pub enum Mode {
    /// The centre of what is played most and liked.
    Taste,
    Energetic,
    Calm,
    Sad,
    /// Like this track (its id).
    LikeTrack(i64),
}

/// The candidates with their places in the mood space, built once per wave (and again when
/// the library's analysis grows).
pub struct Pool {
    candidates: Vec<Candidate>,
    points: Vec<Point>,
    index: HashMap<i64, usize>,
    habits: Habits,
}

/// Each value's rank among all of them, 0 (lowest) to 1 (highest); ties share a rank.
fn ranks(values: &[f32]) -> Vec<f32> {
    let n = values.len();
    if n < 2 {
        return vec![0.5; n];
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut ranks = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && values[order[j + 1]] == values[order[i]] {
            j += 1;
        }
        let rank = (i + j) as f32 / 2.0 / (n - 1) as f32;
        for &k in &order[i..=j] {
            ranks[k] = rank;
        }
        i = j + 1;
    }
    ranks
}

impl Pool {
    pub fn new(candidates: Vec<Candidate>, habits: Habits) -> Self {
        let column = |f: fn(&Features) -> f32| -> Vec<f32> {
            ranks(
                &candidates
                    .iter()
                    .map(|c| f(&c.features))
                    .collect::<Vec<_>>(),
            )
        };
        let energy = column(|f| f.energy);
        let tempo = column(|f| f.tempo);
        let brightness = column(|f| f.brightness);
        let valence = column(|f| f.valence);
        let spread = column(|f| f.spread);
        let points = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                // An unclear beat says little about pace: towards the middle.
                let confidence = c.features.tempo_confidence.clamp(0.0, 1.0);
                let pace = 0.5 + (tempo[i] - 0.5) * (0.3 + 0.7 * confidence);
                [energy[i], pace, brightness[i], valence[i], spread[i]]
            })
            .collect();
        let index = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect();
        Self {
            candidates,
            points,
            index,
            habits,
        }
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub fn contains(&self, id: i64) -> bool {
        self.index.contains_key(&id)
    }

    /// A track's place in the mood space.
    pub fn point(&self, id: i64) -> Option<Point> {
        self.index.get(&id).map(|&i| self.points[i])
    }

    fn candidate(&self, id: i64) -> Option<&Candidate> {
        self.index.get(&id).map(|&i| &self.candidates[i])
    }

    /// Where the listener's taste sits: plays and favorites weighed, the middle without them.
    fn taste(&self) -> Point {
        let mut sum = [0.0; DIMS];
        let mut weight = 0.0;
        for (c, point) in self.candidates.iter().zip(&self.points) {
            let w = (c.plays as f32).ln_1p() + if c.favorite { 2.0 } else { 0.0 };
            if w > 0.0 {
                for (s, p) in sum.iter_mut().zip(point) {
                    *s += w * p;
                }
                weight += w;
            }
        }
        if weight > 0.0 {
            sum.map(|s| s / weight)
        } else {
            [0.5; DIMS]
        }
    }
}

/// Where each mood starts in the space.
fn mood_point(mode: Mode, pool: &Pool) -> Point {
    match mode {
        Mode::Taste => pool.taste(),
        Mode::Energetic => [0.85, 0.8, 0.75, 0.6, 0.5],
        Mode::Calm => [0.2, 0.25, 0.35, 0.55, 0.25],
        Mode::Sad => [0.3, 0.25, 0.25, 0.12, 0.4],
        Mode::LikeTrack(id) => pool.point(id).unwrap_or_else(|| pool.taste()),
    }
}

/// xorshift64*: deterministic for tests, plenty for picking songs.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Standard normal (Box–Muller).
    fn normal(&mut self) -> f32 {
        let u = self.unit().max(1e-7);
        let v = self.unit();
        (-2.0 * u.ln()).sqrt() * (2.0 * std::f32::consts::PI * v).cos()
    }
}

/// A pick and how it was chosen (the caller asks the library for more when closeness is low).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    pub id: i64,
    /// How close to the course it sounds, 0–1.
    pub closeness: f32,
}

/// One wave: its mood, course and what it played. Saved with the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wave {
    pub mode: Mode,
    course: Point,
    anchor: Point,
    /// This wave's picks, in order.
    picked: Vec<i64>,
    /// Liked in this wave: the course stays near them.
    liked: Vec<i64>,
    disliked: Vec<i64>,
    hold: u32,
    rng: Rng,
}

fn distance(a: &Point, b: &Point) -> f32 {
    a.iter()
        .zip(b)
        .zip(&WEIGHTS)
        .map(|((x, y), w)| w * (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

fn clamp_point(p: Point) -> Point {
    p.map(|v| v.clamp(0.0, 1.0))
}

impl Wave {
    pub fn new(mode: Mode, pool: &Pool, seed: u64) -> Self {
        let anchor = mood_point(mode, pool);
        Self {
            mode,
            course: anchor,
            anchor,
            picked: Vec::new(),
            liked: Vec::new(),
            disliked: Vec::new(),
            hold: 0,
            rng: Rng::new(seed),
        }
    }

    pub fn course(&self) -> Point {
        self.course
    }

    pub fn picked(&self) -> &[i64] {
        &self.picked
    }

    /// Chooses the next track, or `None` when nothing is left to choose from.
    pub fn pick(&mut self, pool: &Pool, now: i64) -> Option<Pick> {
        let previous = self.picked.last().and_then(|id| pool.candidate(*id));
        let previous_artists: HashSet<i64> = previous
            .map(|c| c.artists.iter().copied().collect())
            .unwrap_or_default();
        let played: HashSet<i64> = self.picked.iter().copied().collect();
        let disliked: HashSet<i64> = self.disliked.iter().copied().collect();

        // What the wave has played so far, for affinity.
        let mut artist_counts: HashMap<i64, u32> = HashMap::new();
        let mut genre_counts: HashMap<&str, u32> = HashMap::new();
        let mut years = Vec::new();
        for c in self.picked.iter().filter_map(|id| pool.candidate(*id)) {
            for a in &c.artists {
                *artist_counts.entry(*a).or_default() += 1;
            }
            if let Some(g) = &c.genre {
                *genre_counts.entry(g.as_str()).or_default() += 1;
            }
            years.extend(c.year);
        }
        let year_mean = (!years.is_empty())
            .then(|| years.iter().map(|y| *y as f32).sum::<f32>() / years.len() as f32);
        let wave_len = self.picked.len().max(1) as f32;

        let eligible = |c: &Candidate, strict: bool| {
            !played.contains(&c.id)
                && !disliked.contains(&c.id)
                && !pool.habits.dislikes.contains(&c.id)
                && (!strict
                    || (c.artists.iter().all(|a| !previous_artists.contains(a))
                        && c.last_played.is_none_or(|t| now - t >= RECENT_SECONDS)))
        };
        let mut scored: Vec<(f32, f32, i64)> = Vec::new();
        for strict in [true, false] {
            for (c, point) in pool.candidates.iter().zip(&pool.points) {
                if !eligible(c, strict) {
                    continue;
                }
                let d = distance(point, &self.course);
                let closeness = (-(d * d) / (2.0 * CLOSENESS_WIDTH * CLOSENESS_WIDTH)).exp();
                let mut score = 2.0 * closeness;
                let artist = c
                    .artists
                    .iter()
                    .filter_map(|a| artist_counts.get(a))
                    .max()
                    .copied()
                    .unwrap_or(0);
                score += 0.1 * (artist.min(3) as f32 / 3.0);
                if let Some(g) = &c.genre {
                    score +=
                        0.1 * genre_counts.get(g.as_str()).copied().unwrap_or(0) as f32 / wave_len;
                }
                if let (Some(mean), Some(year)) = (year_mean, c.year) {
                    score += 0.05 * (-(mean - year as f32).abs() / 8.0).exp();
                }
                if let Some(prev) = previous {
                    let after = pool
                        .habits
                        .transitions
                        .get(&(prev.id, c.id))
                        .copied()
                        .unwrap_or(0);
                    score += 0.15 * (after.min(3) as f32 / 3.0);
                }
                if c.favorite {
                    score += 0.15;
                }
                score -= 0.25
                    * pool
                        .habits
                        .skips
                        .get(&c.id)
                        .copied()
                        .unwrap_or(0.0)
                        .min(1.0);
                if let Some(t) = c.last_played {
                    let age = now - t;
                    if age < STALE_SECONDS {
                        score -= 0.2 * (1.0 - age.max(0) as f32 / STALE_SECONDS as f32);
                    }
                }
                scored.push((score, closeness, c.id));
            }
            // A small library or a long wave may leave nothing under the strict rules: then
            // repeating an artist or a recent track is better than stopping.
            if !scored.is_empty() {
                break;
            }
        }
        if scored.is_empty() {
            return None;
        }
        let keep = SHORTLIST.min(scored.len());
        if scored.len() > keep {
            scored.select_nth_unstable_by(keep - 1, |a, b| b.0.total_cmp(&a.0));
            scored.truncate(keep);
        }
        let best = scored.iter().map(|s| s.0).fold(f32::MIN, f32::max);
        let weights: Vec<f32> = scored
            .iter()
            .map(|s| ((s.0 - best) / TEMPERATURE).exp())
            .collect();
        let total: f32 = weights.iter().sum();
        let mut target = self.rng.unit() * total;
        let mut chosen = scored[scored.len() - 1];
        for (s, w) in scored.iter().zip(&weights) {
            if target < *w {
                chosen = *s;
                break;
            }
            target -= w;
        }
        self.picked.push(chosen.2);
        self.step();
        Some(Pick {
            id: chosen.2,
            closeness: chosen.1,
        })
    }

    /// After each pick: a small random step (none while a liked track holds the course), a
    /// pull back towards the mood, and never further from it than `MAX_DRIFT`.
    fn step(&mut self) {
        if self.hold > 0 {
            self.hold -= 1;
        } else {
            for v in &mut self.course {
                *v += WALK * self.rng.normal();
            }
        }
        for (v, a) in self.course.iter_mut().zip(&self.anchor) {
            *v += 0.05 * (a - *v);
        }
        self.bound();
    }

    fn bound(&mut self) {
        let away = distance(&self.course, &self.anchor);
        if away > MAX_DRIFT {
            let keep = MAX_DRIFT / away;
            for (v, a) in self.course.iter_mut().zip(&self.anchor) {
                *v = a + (*v - a) * keep;
            }
        }
        self.course = clamp_point(self.course);
    }

    fn towards(&mut self, point: &Point, share: f32) {
        for (v, p) in self.course.iter_mut().zip(point) {
            *v += share * (p - *v);
        }
        self.bound();
    }

    /// A track was listened to (it played through or mostly): the course leans its way.
    pub fn listened(&mut self, pool: &Pool, id: i64) {
        if let Some(point) = pool.point(id) {
            self.towards(&point, 0.25);
        }
    }

    /// Skipped early: the course leans a little away.
    pub fn skipped(&mut self, pool: &Pool, id: i64) {
        if let Some(point) = pool.point(id) {
            self.towards(&point, -0.1);
        }
    }

    /// "Like": the course goes most of the way there and stays a while.
    pub fn liked(&mut self, pool: &Pool, id: i64) {
        if let Some(point) = pool.point(id) {
            self.towards(&point, 0.5);
            self.hold = HOLD_PICKS;
            self.liked.push(id);
        }
    }

    /// "Don't like": never again in this wave, and the course moves away from it.
    pub fn disliked(&mut self, pool: &Pool, id: i64) {
        self.disliked.push(id);
        self.hold = 0;
        if let Some(point) = pool.point(id) {
            self.towards(&point, -0.3);
        }
    }

    /// The upcoming picks taken back (the queue's tracks after the playing one were removed,
    /// so they can come again).
    pub fn forget(&mut self, ids: &[i64]) {
        self.picked.retain(|id| !ids.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn features(energy: f32, tempo: f32, brightness: f32, valence: f32) -> Features {
        Features {
            energy,
            spread: 4.0,
            tempo,
            tempo_confidence: 0.9,
            brightness,
            valence,
        }
    }

    /// A library of `n` tracks spread evenly over the space, `artists` of them, one in ten a
    /// favorite.
    fn library(n: usize, artists: i64) -> Vec<Candidate> {
        let mut rng = Rng::new(7);
        (0..n)
            .map(|i| Candidate {
                id: i as i64 + 1,
                features: features(
                    -40.0 + 35.0 * rng.unit(),
                    70.0 + 110.0 * rng.unit(),
                    500.0 + 3000.0 * rng.unit(),
                    -0.4 + 0.8 * rng.unit(),
                ),
                artists: vec![i as i64 % artists + 1],
                genre: Some(["rock", "pop", "jazz"][i % 3].to_owned()),
                year: Some(1970 + (i % 50) as i32),
                favorite: i % 10 == 0,
                plays: (i % 7) as u32,
                last_played: None,
            })
            .collect()
    }

    fn run(wave: &mut Wave, pool: &Pool, picks: usize) -> Vec<i64> {
        (0..picks)
            .map(|_| wave.pick(pool, 1_000_000).expect("a pick").id)
            .collect()
    }

    fn mean(pool: &Pool, ids: &[i64], axis: usize) -> f32 {
        ids.iter()
            .map(|id| pool.point(*id).unwrap()[axis])
            .sum::<f32>()
            / ids.len() as f32
    }

    #[test]
    fn ranks_are_shares_of_the_library() {
        assert_eq!(ranks(&[3.0, 1.0, 2.0]), vec![1.0, 0.0, 0.5]);
        assert_eq!(ranks(&[1.0, 1.0, 2.0]), vec![0.25, 0.25, 1.0]);
        assert_eq!(ranks(&[5.0]), vec![0.5]);
    }

    #[test]
    fn moods_pick_their_corner() {
        let pool = Pool::new(library(600, 60), Habits::default());
        let energetic = run(&mut Wave::new(Mode::Energetic, &pool, 1), &pool, 30);
        let calm = run(&mut Wave::new(Mode::Calm, &pool, 1), &pool, 30);
        let sad = run(&mut Wave::new(Mode::Sad, &pool, 1), &pool, 30);
        assert!(
            mean(&pool, &energetic, 0) > 0.7,
            "{}",
            mean(&pool, &energetic, 0)
        );
        assert!(mean(&pool, &energetic, 1) > 0.6);
        assert!(mean(&pool, &calm, 0) < 0.35, "{}", mean(&pool, &calm, 0));
        assert!(mean(&pool, &calm, 1) < 0.4);
        assert!(mean(&pool, &sad, 3) < 0.3, "{}", mean(&pool, &sad, 3));
    }

    #[test]
    fn like_a_track_starts_next_to_it() {
        let pool = Pool::new(library(600, 60), Habits::default());
        let seed_point = pool.point(42).unwrap();
        let mut wave = Wave::new(Mode::LikeTrack(42), &pool, 3);
        let first = wave.pick(&pool, 0).unwrap();
        assert!(distance(&pool.point(first.id).unwrap(), &seed_point) < 0.25);
        assert!(first.closeness > 0.5);
    }

    #[test]
    fn taste_starts_where_the_plays_and_favorites_are() {
        let mut tracks = library(300, 30);
        for c in &mut tracks {
            let loud = c.features.energy > -15.0;
            c.plays = if loud { 20 } else { 0 };
            c.favorite = loud;
        }
        let pool = Pool::new(tracks, Habits::default());
        let picks = run(&mut Wave::new(Mode::Taste, &pool, 5), &pool, 20);
        assert!(mean(&pool, &picks, 0) > 0.6, "{}", mean(&pool, &picks, 0));
    }

    #[test]
    fn no_artist_twice_in_a_row_and_no_repeats() {
        let pool = Pool::new(library(200, 8), Habits::default());
        let mut wave = Wave::new(Mode::Taste, &pool, 11);
        let picks = run(&mut wave, &pool, 150);
        let unique: HashSet<_> = picks.iter().collect();
        assert_eq!(unique.len(), picks.len());
        for pair in picks.windows(2) {
            let a = pool.candidate(pair[0]).unwrap();
            let b = pool.candidate(pair[1]).unwrap();
            assert_ne!(a.artists[0], b.artists[0]);
        }
    }

    #[test]
    fn a_tiny_library_still_plays_and_then_ends() {
        let pool = Pool::new(library(3, 1), Habits::default());
        let mut wave = Wave::new(Mode::Calm, &pool, 2);
        assert_eq!(run(&mut wave, &pool, 3).len(), 3);
        assert_eq!(wave.pick(&pool, 0), None);
    }

    #[test]
    fn dislikes_steer_away_and_never_come_back() {
        let pool = Pool::new(library(600, 60), Habits::default());
        let mut wave = Wave::new(Mode::Taste, &pool, 9);
        let first = wave.pick(&pool, 0).unwrap().id;
        let point = pool.point(first).unwrap();
        let before = distance(&wave.course(), &point);
        wave.disliked(&pool, first);
        assert!(distance(&wave.course(), &point) > before);
        // Disliked anywhere (the library's list) is never picked either.
        let habits = Habits {
            dislikes: [7, 8, 9].into(),
            ..Habits::default()
        };
        let pool = Pool::new(library(40, 10), habits);
        let picks = run(&mut Wave::new(Mode::Taste, &pool, 4), &pool, 37);
        assert!(!picks.iter().any(|id| [7, 8, 9].contains(id)));
        assert!(Wave::new(Mode::Taste, &pool, 4).pick(&pool, 0).is_some());
    }

    #[test]
    fn a_like_holds_the_course() {
        let pool = Pool::new(library(600, 60), Habits::default());
        let mut wave = Wave::new(Mode::Energetic, &pool, 13);
        let liked = pool
            .candidates
            .iter()
            .find(|c| pool.point(c.id).unwrap()[0] > 0.6 && pool.point(c.id).unwrap()[3] < 0.3)
            .unwrap()
            .id;
        wave.pick(&pool, 0);
        wave.liked(&pool, liked);
        let target = pool.point(liked).unwrap();
        let near = run(&mut wave, &pool, 3)
            .iter()
            .map(|id| distance(&pool.point(*id).unwrap(), &target))
            .fold(0.0, f32::max);
        assert!(near < 0.45, "{near}");
    }

    #[test]
    fn the_course_drifts_but_stays_in_its_mood() {
        let pool = Pool::new(library(2000, 200), Habits::default());
        let mut wave = Wave::new(Mode::Calm, &pool, 21);
        let anchor = wave.anchor;
        let mut moved = 0.0f32;
        for _ in 0..1000 {
            let Some(pick) = wave.pick(&pool, 0) else {
                break;
            };
            wave.listened(&pool, pick.id);
            let away = distance(&wave.course(), &anchor);
            assert!(away <= MAX_DRIFT + 1e-4, "{away}");
            moved = moved.max(away);
        }
        assert!(moved > 0.05, "the course never moved: {moved}");
    }

    #[test]
    fn habits_count() {
        let mut tracks = library(100, 50);
        for c in &mut tracks {
            c.features = features(-20.0, 120.0, 1500.0, 0.0);
            c.favorite = false;
            c.genre = None;
            c.year = None;
        }
        // All alike: the one usually played after the first wins about e times as often as
        // any other of the shortlist (e / (e + 11) of 200 ≈ 40); a skipped one never makes it.
        let habits = Habits {
            transitions: [((1, 50), 5)].into(),
            skips: [(51, 3.0)].into(),
            ..Habits::default()
        };
        let pool = Pool::new(tracks, habits);
        let mut follows = 0;
        let mut skipped = 0;
        for seed in 0..200 {
            let mut wave = Wave::new(Mode::Taste, &pool, seed);
            wave.picked.push(1);
            let next = wave.pick(&pool, 0).unwrap().id;
            follows += i32::from(next == 50);
            skipped += i32::from(next == 51);
        }
        assert!(follows > 25, "{follows}");
        assert_eq!(skipped, 0);
    }

    #[test]
    fn recent_tracks_wait() {
        let mut tracks = library(50, 50);
        for c in &mut tracks {
            c.last_played = Some(1_000_000 - 60);
        }
        tracks[10].last_played = None;
        let pool = Pool::new(tracks, Habits::default());
        // Only one wasn't played a minute ago: it comes first.
        let first = Wave::new(Mode::Taste, &pool, 1)
            .pick(&pool, 1_000_000)
            .unwrap();
        assert_eq!(first.id, 11);
    }

    #[test]
    fn ten_thousand_tracks_pick_quickly() {
        let pool = Pool::new(library(10_000, 1000), Habits::default());
        let mut wave = Wave::new(Mode::Taste, &pool, 17);
        let start = Instant::now();
        for _ in 0..50 {
            wave.pick(&pool, 0).unwrap();
        }
        let each = start.elapsed() / 50;
        assert!(each.as_millis() < 10, "{each:?} a pick");
    }

    #[test]
    fn a_wave_survives_the_session() {
        let pool = Pool::new(library(100, 20), Habits::default());
        let mut wave = Wave::new(Mode::Sad, &pool, 5);
        run(&mut wave, &pool, 5);
        let saved = serde_json::to_string(&wave).unwrap();
        let mut back: Wave = serde_json::from_str(&saved).unwrap();
        // The same state: the same next pick.
        assert_eq!(back.pick(&pool, 0), wave.pick(&pool, 0));
        assert_eq!(
            serde_json::to_value(Mode::LikeTrack(4)).unwrap(),
            serde_json::json!({ "kind": "likeTrack", "track": 4 })
        );
    }
}
