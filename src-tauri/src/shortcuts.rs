//! Keyboard shortcuts the user changed (`settings.shortcuts`): an action id (`play-pause`)
//! with its key combination (`Mod+Alt+KeyS`), or `None` for an action the user took the
//! shortcut off. The frontend owns the actions, their defaults and the matching
//! (`src/shortcuts`); this only keeps the map tidy.

use std::collections::BTreeMap;

pub type Shortcuts = BTreeMap<String, Option<String>>;

/// Far more than there are actions; a bigger map is not something the app wrote.
const MAX_ENTRIES: usize = 100;
const MAX_LEN: usize = 64;

fn is_action(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Modifiers and a key code joined by `+`: letters, digits and `+` only.
fn is_combo(combo: &str) -> bool {
    !combo.is_empty()
        && combo.len() <= MAX_LEN
        && combo.chars().all(|c| c.is_ascii_alphanumeric() || c == '+')
}

/// Drops entries that can't be an action and its combination.
pub fn sanitize(shortcuts: Shortcuts) -> Shortcuts {
    shortcuts
        .into_iter()
        .filter(|(id, combo)| is_action(id) && combo.as_deref().is_none_or(is_combo))
        .take(MAX_ENTRIES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_combinations_and_removed_shortcuts() {
        let shortcuts = Shortcuts::from([
            ("play-pause".to_owned(), Some("KeyK".to_owned())),
            ("mute".to_owned(), None),
            ("sidebar".to_owned(), Some("Mod+Alt+KeyS".to_owned())),
        ]);
        assert_eq!(sanitize(shortcuts.clone()), shortcuts);
    }

    #[test]
    fn drops_what_is_not_an_action_and_a_combination() {
        let shortcuts = Shortcuts::from([
            ("Play".to_owned(), Some("KeyK".to_owned())),
            (String::new(), None),
            ("next".to_owned(), Some("Mod+<script>".to_owned())),
            ("previous".to_owned(), Some(String::new())),
            ("x".repeat(65), None),
            ("search".to_owned(), Some("Mod+KeyF".to_owned())),
        ]);
        assert_eq!(
            sanitize(shortcuts),
            Shortcuts::from([("search".to_owned(), Some("Mod+KeyF".to_owned()))])
        );
    }

    #[test]
    fn keeps_a_bounded_number() {
        let shortcuts: Shortcuts = (0..500).map(|n| (format!("action-{n}"), None)).collect();
        assert_eq!(sanitize(shortcuts).len(), MAX_ENTRIES);
    }
}
