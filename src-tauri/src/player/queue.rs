//! The play queue: what plays now, what comes next, shuffle and repeat. Pure logic; the
//! player mirrors its decisions into mpv.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Something to play, with what's known about it for display before its tags are read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    /// Set for library tracks: plays are counted and shown in history.
    pub track_id: Option<i64>,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<f64>,
    /// Library cover id.
    pub cover: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    /// Unique within the queue, so the same track can be queued twice.
    pub uid: u64,
    #[serde(flatten)]
    pub entry: Entry,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

/// What the windows see.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueView {
    pub items: Vec<QueueItem>,
    pub current: Option<usize>,
    pub shuffle: bool,
    pub repeat: Repeat,
}

/// Items in play order. Items before `current` were played; the ones after it are up next.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    items: Vec<QueueItem>,
    current: Option<usize>,
    shuffle: bool,
    repeat: Repeat,
    /// The order before shuffling (uids), restored when shuffle is turned off. Kept in sync
    /// with `items` while shuffle is on, empty otherwise.
    #[serde(default)]
    unshuffled: Vec<u64>,
    #[serde(default)]
    next_uid: u64,
}

/// Fisher–Yates with xorshift64; the queue doesn't need a cryptographic generator.
fn shuffle_slice<T>(items: &mut [T], seed: u64) {
    let mut state = seed | 1;
    for i in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

impl Queue {
    pub fn view(&self) -> QueueView {
        QueueView {
            items: self.items.clone(),
            current: self.current,
            shuffle: self.shuffle,
            repeat: self.repeat,
        }
    }

    pub fn items(&self) -> &[QueueItem] {
        &self.items
    }

    pub fn current(&self) -> Option<&QueueItem> {
        self.current.and_then(|index| self.items.get(index))
    }

    fn make(&mut self, entries: Vec<Entry>) -> Vec<QueueItem> {
        entries
            .into_iter()
            .map(|entry| {
                self.next_uid += 1;
                QueueItem {
                    uid: self.next_uid,
                    entry,
                }
            })
            .collect()
    }

    fn index_of(&self, uid: u64) -> Option<usize> {
        self.items.iter().position(|item| item.uid == uid)
    }

    /// Replaces the queue, starting at `start` (with shuffle on, that item plays first and
    /// the rest are shuffled).
    pub fn replace(&mut self, entries: Vec<Entry>, start: usize, seed: u64) {
        self.items = self.make(entries);
        self.unshuffled.clear();
        if self.items.is_empty() {
            self.current = None;
            return;
        }
        self.current = Some(start.min(self.items.len() - 1));
        if self.shuffle {
            self.shuffle = false;
            self.set_shuffle(true, seed);
        }
    }

    /// Inserts entries right after the current item.
    pub fn play_next(&mut self, entries: Vec<Entry>) {
        let new = self.make(entries);
        let at = self.current.map_or(0, |index| index + 1);
        if self.shuffle {
            let after = self
                .current()
                .and_then(|item| self.unshuffled.iter().position(|uid| *uid == item.uid))
                .map_or(0, |index| index + 1);
            self.unshuffled
                .splice(after..after, new.iter().map(|item| item.uid));
        }
        self.items.splice(at..at, new);
    }

    pub fn append(&mut self, entries: Vec<Entry>) {
        let new = self.make(entries);
        if self.shuffle {
            self.unshuffled.extend(new.iter().map(|item| item.uid));
        }
        self.items.extend(new);
    }

    /// Removes items; the current one stays (it is playing).
    pub fn remove(&mut self, uids: &[u64]) {
        let current = self.current().map(|item| item.uid);
        let gone = |uid: u64| uids.contains(&uid) && Some(uid) != current;
        self.items.retain(|item| !gone(item.uid));
        self.unshuffled.retain(|uid| !gone(*uid));
        self.current = current.and_then(|uid| self.index_of(uid));
    }

    /// Removes everything after the current item.
    pub fn clear_upcoming(&mut self) {
        let upcoming: Vec<u64> = match self.current {
            Some(index) => self.items[index + 1..].iter().map(|i| i.uid).collect(),
            None => self.items.iter().map(|i| i.uid).collect(),
        };
        self.remove(&upcoming);
    }

    /// Moves the item at `from` to index `to`. The current item keeps playing wherever it
    /// lands.
    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        if from >= self.items.len() {
            return false;
        }
        let current = self.current().map(|item| item.uid);
        let item = self.items.remove(from);
        self.items.insert(to.min(self.items.len()), item);
        self.current = current.and_then(|uid| self.index_of(uid));
        true
    }

    /// Makes the item with `uid` current.
    pub fn jump(&mut self, uid: u64) -> Option<&QueueItem> {
        self.current = Some(self.index_of(uid)?);
        self.current()
    }

    /// The item that follows when the current one ends by itself: what mpv preloads for
    /// gapless playback.
    pub fn peek_next(&self) -> Option<&QueueItem> {
        self.next_index(false)
            .and_then(|index| self.items.get(index))
    }

    fn next_index(&self, manual: bool) -> Option<usize> {
        let current = self.current?;
        if !manual && self.repeat == Repeat::One {
            Some(current)
        } else if current + 1 < self.items.len() {
            Some(current + 1)
        } else if self.repeat != Repeat::Off {
            Some(0)
        } else {
            None
        }
    }

    /// Moves to the next item. `manual` (the Next button) moves on even with repeat-one.
    /// `None` at the end of the queue; the last item stays current.
    pub fn advance(&mut self, manual: bool) -> Option<&QueueItem> {
        let next = self.next_index(manual)?;
        self.current = Some(next);
        self.current()
    }

    /// Moves to the previous item (wrapping with repeat on); `None` at the start.
    pub fn back(&mut self) -> Option<&QueueItem> {
        let current = self.current?;
        let previous = if current > 0 {
            current - 1
        } else if self.repeat != Repeat::Off {
            self.items.len() - 1
        } else {
            return None;
        };
        self.current = Some(previous);
        self.current()
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    /// Shuffle puts the current item first and shuffles everything else; turning it off
    /// restores the original order (with anything added meanwhile in place).
    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        if on == self.shuffle {
            return;
        }
        self.shuffle = on;
        if on {
            self.unshuffled = self.items.iter().map(|item| item.uid).collect();
            let current = self.current.map(|index| self.items.remove(index));
            shuffle_slice(&mut self.items, seed);
            if let Some(item) = current {
                self.items.insert(0, item);
                self.current = Some(0);
            }
        } else {
            let current = self.current().map(|item| item.uid);
            let order: HashMap<u64, usize> = self
                .unshuffled
                .iter()
                .enumerate()
                .map(|(index, uid)| (*uid, index))
                .collect();
            self.items
                .sort_by_key(|item| order.get(&item.uid).copied().unwrap_or(usize::MAX));
            self.unshuffled.clear();
            self.current = current.and_then(|uid| self.index_of(uid));
        }
    }

    /// Updates the display info of every item playing `path` (e.g. after a tag edit).
    pub fn update_entries(&mut self, path: &str, update: impl Fn(&mut Entry)) -> bool {
        self.update_where(|entry| entry.path == path, update)
    }

    /// Updates every entry `matches` picks; returns whether there was one.
    pub fn update_where(
        &mut self,
        matches: impl Fn(&Entry) -> bool,
        update: impl Fn(&mut Entry),
    ) -> bool {
        let mut changed = false;
        for item in self.items.iter_mut().filter(|item| matches(&item.entry)) {
            update(&mut item.entry);
            changed = true;
        }
        changed
    }

    /// Repairs a queue loaded from storage (written by an older or crashed build).
    pub fn sanitize(&mut self) {
        let max = self.items.iter().map(|item| item.uid).max().unwrap_or(0);
        self.next_uid = self.next_uid.max(max);
        let mut seen = std::collections::HashSet::new();
        let current = self.current().map(|item| item.uid);
        self.items.retain(|item| seen.insert(item.uid));
        self.current = current.and_then(|uid| self.index_of(uid));
        if self.current.is_none() && !self.items.is_empty() {
            self.current = Some(0);
        }
        if self.shuffle {
            self.unshuffled.retain(|uid| seen.contains(uid));
            for item in &self.items {
                if !self.unshuffled.contains(&item.uid) {
                    self.unshuffled.push(item.uid);
                }
            }
        } else {
            self.unshuffled.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn entries(names: &[&str]) -> Vec<Entry> {
        names
            .iter()
            .map(|name| Entry {
                path: format!("/{name}.flac"),
                title: (*name).into(),
                ..Entry::default()
            })
            .collect()
    }

    fn titles(queue: &Queue) -> Vec<&str> {
        queue
            .items()
            .iter()
            .map(|item| item.entry.title.as_str())
            .collect()
    }

    fn current(queue: &Queue) -> Option<&str> {
        queue.current().map(|item| item.entry.title.as_str())
    }

    fn queue(names: &[&str], start: usize) -> Queue {
        let mut queue = Queue::default();
        queue.replace(entries(names), start, 1);
        queue
    }

    /// Checks the invariants every operation must keep.
    fn check(queue: &Queue) {
        let uids: HashSet<u64> = queue.items.iter().map(|i| i.uid).collect();
        assert_eq!(uids.len(), queue.items.len(), "unique uids");
        if let Some(index) = queue.current {
            assert!(index < queue.items.len());
        }
        if queue.shuffle {
            let original: HashSet<u64> = queue.unshuffled.iter().copied().collect();
            assert_eq!(original, uids, "unshuffled order covers the items");
            assert_eq!(original.len(), queue.unshuffled.len());
        } else {
            assert!(queue.unshuffled.is_empty());
        }
        assert!(queue.items.iter().all(|i| i.uid <= queue.next_uid));
    }

    #[test]
    fn replace_starts_at_the_chosen_item() {
        let queue = queue(&["a", "b", "c"], 1);
        assert_eq!(current(&queue), Some("b"));
        let mut empty = queue.clone();
        empty.replace(Vec::new(), 0, 1);
        assert_eq!(empty.current(), None);
        let clamped = self::queue(&["a", "b"], 9);
        assert_eq!(current(&clamped), Some("b"));
    }

    #[test]
    fn plays_through_and_stops_without_repeat() {
        let mut queue = queue(&["a", "b"], 0);
        assert_eq!(queue.peek_next().map(|i| i.entry.title.as_str()), Some("b"));
        assert_eq!(
            queue.advance(false).map(|i| i.uid),
            Some(queue.items[1].uid)
        );
        assert!(queue.peek_next().is_none());
        assert!(queue.advance(false).is_none());
        assert_eq!(current(&queue), Some("b"), "the last item stays current");
    }

    #[test]
    fn repeat_all_wraps_and_repeat_one_stays() {
        let mut queue = queue(&["a", "b"], 1);
        queue.set_repeat(Repeat::All);
        assert_eq!(queue.peek_next().map(|i| i.entry.title.as_str()), Some("a"));
        queue.advance(false);
        assert_eq!(current(&queue), Some("a"));

        queue.set_repeat(Repeat::One);
        assert_eq!(queue.peek_next().map(|i| i.entry.title.as_str()), Some("a"));
        queue.advance(false);
        assert_eq!(current(&queue), Some("a"));
        // The Next button still moves on.
        queue.advance(true);
        assert_eq!(current(&queue), Some("b"));
    }

    #[test]
    fn back_stops_at_the_start_unless_repeating() {
        let mut queue = queue(&["a", "b"], 1);
        assert_eq!(
            queue.back().map(|i| i.entry.title.clone()),
            Some("a".into())
        );
        assert!(queue.back().is_none());
        queue.set_repeat(Repeat::All);
        assert_eq!(
            queue.back().map(|i| i.entry.title.clone()),
            Some("b".into())
        );
    }

    #[test]
    fn play_next_and_append() {
        let mut queue = queue(&["a", "b", "c"], 0);
        queue.play_next(entries(&["x", "y"]));
        queue.append(entries(&["z"]));
        assert_eq!(titles(&queue), ["a", "x", "y", "b", "c", "z"]);
        assert_eq!(current(&queue), Some("a"));
        check(&queue);

        let mut empty = Queue::default();
        empty.play_next(entries(&["x"]));
        assert_eq!(titles(&empty), ["x"]);
        assert_eq!(empty.current(), None);
    }

    #[test]
    fn the_same_track_can_be_queued_twice() {
        let mut queue = queue(&["a"], 0);
        queue.append(entries(&["a"]));
        assert_eq!(titles(&queue), ["a", "a"]);
        assert_ne!(queue.items[0].uid, queue.items[1].uid);
        let second = queue.items[1].uid;
        queue.remove(&[second]);
        assert_eq!(titles(&queue), ["a"]);
    }

    #[test]
    fn remove_keeps_the_current_item() {
        let mut queue = queue(&["a", "b", "c", "d"], 2);
        let uids: Vec<u64> = queue.items.iter().map(|i| i.uid).collect();
        queue.remove(&[uids[0], uids[2], uids[3]]);
        assert_eq!(titles(&queue), ["b", "c"]);
        assert_eq!(current(&queue), Some("c"));

        queue.clear_upcoming();
        assert_eq!(titles(&queue), ["b", "c"]);
        queue.jump(uids[1]);
        queue.clear_upcoming();
        assert_eq!(titles(&queue), ["b"]);
    }

    #[test]
    fn moving_items_keeps_the_current_one_playing() {
        let mut queue = queue(&["a", "b", "c", "d"], 1);
        assert!(queue.move_item(3, 0));
        assert_eq!(titles(&queue), ["d", "a", "b", "c"]);
        assert_eq!(current(&queue), Some("b"));
        assert!(queue.move_item(2, 99));
        assert_eq!(titles(&queue), ["d", "a", "c", "b"]);
        assert_eq!(current(&queue), Some("b"));
        assert!(!queue.move_item(4, 0));
    }

    #[test]
    fn shuffle_puts_the_current_item_first_and_unshuffle_restores_the_order() {
        let names: Vec<String> = (0..20).map(|i| format!("t{i}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut queue = queue(&names, 5);

        queue.set_shuffle(true, 42);
        check(&queue);
        assert_eq!(current(&queue), Some("t5"));
        assert_eq!(queue.current, Some(0));
        assert_ne!(titles(&queue)[1..], names[..19], "shuffled");
        let mut sorted = titles(&queue);
        sorted.sort_unstable();
        let mut expected = names.clone();
        expected.sort_unstable();
        assert_eq!(sorted, expected, "same items");

        queue.advance(false);
        let playing = current(&queue).map(str::to_owned);
        queue.set_shuffle(false, 0);
        check(&queue);
        assert_eq!(titles(&queue), names);
        assert_eq!(current(&queue).map(str::to_owned), playing);
    }

    #[test]
    fn shuffled_replace_starts_with_the_chosen_item() {
        let mut queue = Queue::default();
        queue.set_shuffle(true, 3);
        queue.replace(entries(&["a", "b", "c", "d", "e"]), 3, 9);
        check(&queue);
        assert_eq!(current(&queue), Some("d"));
        assert_eq!(queue.current, Some(0));
        queue.set_shuffle(false, 0);
        assert_eq!(titles(&queue), ["a", "b", "c", "d", "e"]);
        assert_eq!(current(&queue), Some("d"));
    }

    #[test]
    fn items_added_while_shuffled_land_sensibly_when_unshuffled() {
        let mut queue = queue(&["a", "b", "c", "d"], 1);
        queue.set_shuffle(true, 7);
        queue.play_next(entries(&["next"]));
        queue.append(entries(&["last"]));
        assert_eq!(titles(&queue)[..2], ["b", "next"]);
        queue.set_shuffle(false, 0);
        assert_eq!(titles(&queue), ["a", "b", "next", "c", "d", "last"]);
        assert_eq!(current(&queue), Some("b"));
    }

    #[test]
    fn updates_entries_by_path() {
        let mut queue = queue(&["a", "b"], 0);
        queue.append(entries(&["a"]));
        let changed = queue.update_entries("/a.flac", |entry| entry.title = "A!".into());
        assert!(changed);
        assert_eq!(titles(&queue), ["A!", "b", "A!"]);
        assert!(!queue.update_entries("/zzz.flac", |_| {}));
    }

    #[test]
    fn updates_entries_by_track() {
        let mut queue = queue(&["a", "b"], 0);
        queue.items[1].entry.track_id = Some(7);
        let changed = queue.update_where(
            |entry| entry.track_id == Some(7),
            |entry| entry.track_id = None,
        );
        assert!(changed);
        assert!(queue.items.iter().all(|item| item.entry.track_id.is_none()));
    }

    #[test]
    fn round_trips_through_json_and_sanitizes() {
        let mut queue = queue(&["a", "b", "c"], 1);
        queue.set_shuffle(true, 5);
        queue.set_repeat(Repeat::All);
        let json = serde_json::to_string(&queue).unwrap();
        let mut restored: Queue = serde_json::from_str(&json).unwrap();
        restored.sanitize();
        assert_eq!(restored, queue);

        // Damaged: duplicate uids, a current index out of range, a stale unshuffled list.
        restored.items.push(restored.items[0].clone());
        restored.current = Some(10);
        restored.unshuffled.push(999);
        restored.next_uid = 0;
        restored.sanitize();
        check(&restored);
        assert_eq!(restored.items.len(), 3);
        assert_eq!(restored.current, Some(0));
        // New uids don't collide with stored ones.
        restored.append(entries(&["d"]));
        check(&restored);
    }

    #[test]
    fn view_serializes_flat_items_in_camel_case() {
        let mut queue = queue(&["a"], 0);
        queue.items[0].entry.track_id = Some(7);
        let json = serde_json::to_value(queue.view()).unwrap();
        assert_eq!(json["items"][0]["trackId"], 7);
        assert_eq!(json["items"][0]["title"], "a");
        assert_eq!(json["current"], 0);
        assert_eq!(json["repeat"], "off");
    }

    /// Random operation sequences keep the invariants.
    #[test]
    fn random_operations_keep_the_invariants() {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut random = move |bound: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % bound.max(1) as u64) as usize
        };
        let mut queue = Queue::default();
        for step in 0..5000 {
            let len = queue.items.len();
            match random(11) {
                0 => queue.replace(entries(&["a", "b", "c", "d"]), random(5), step),
                1 => queue.play_next(entries(&["n"])),
                2 => queue.append(entries(&["e", "f"])),
                3 if len > 0 => {
                    let uid = queue.items[random(len)].uid;
                    queue.remove(&[uid]);
                }
                4 => {
                    queue.move_item(random(len + 1), random(len + 1));
                }
                5 => {
                    queue.advance(random(2) == 0);
                }
                6 => {
                    queue.back();
                }
                7 => queue.set_shuffle(random(2) == 0, step),
                8 => queue.set_repeat([Repeat::Off, Repeat::All, Repeat::One][random(3)]),
                9 if len > 0 => {
                    let uid = queue.items[random(len)].uid;
                    queue.jump(uid);
                }
                10 if random(20) == 0 => queue.clear_upcoming(),
                _ => {}
            }
            check(&queue);
            if let Some(next) = queue.peek_next().map(|i| i.uid) {
                let mut copy = queue.clone();
                assert_eq!(copy.advance(false).map(|i| i.uid), Some(next));
            }
        }
    }
}
