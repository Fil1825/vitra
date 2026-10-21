use rusqlite::{OptionalExtension, params};

use crate::{Library, Result};

impl Library {
    /// A stored session value (JSON written by the app), if any.
    pub fn session_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM session WHERE key = ?", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn session_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Library;

    #[test]
    fn stores_and_replaces_values() {
        let library = Library::open_in_memory().unwrap();
        assert_eq!(library.session_get("queue").unwrap(), None);
        library.session_set("queue", "[1]").unwrap();
        library.session_set("queue", "[1,2]").unwrap();
        assert_eq!(
            library.session_get("queue").unwrap().as_deref(),
            Some("[1,2]")
        );
    }
}
