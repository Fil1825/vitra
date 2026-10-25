//! Opens web pages (the project's page, the donation link) in the default browser, with the
//! system's own opener; no plugin needed for that.

use std::process::Command;

use crate::error::AppError;

/// Only plain web addresses: nothing a shell or an opener could read as an option or a file.
fn web_address(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= 2048
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[tauri::command]
pub fn open_in_browser(url: String) -> Result<(), AppError> {
    if !web_address(&url) {
        return Err(AppError::NotFound(format!("not a web address: {url}")));
    }
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    // Explorer hands a web address to the default browser; `cmd /c start` would read `&`.
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    command.arg(&url).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_are_opened() {
        assert!(web_address("https://github.com/Fil1825/vitra"));
        assert!(web_address("http://example.com/a?b=c&d=e"));
        assert!(!web_address("file:///etc/passwd"));
        assert!(!web_address("-a Calculator"));
        assert!(!web_address("https://example.com/ --x"));
        assert!(!web_address("javascript:alert(1)"));
    }
}
