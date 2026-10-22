//! Links dragged onto the window from a browser. Tauri's drop events carry file paths only
//! (wry reads just the file names off the drag pasteboard and reports every drop as handled,
//! so the page never sees it): a dragged link arrives as a drop without paths. The window then
//! asks for the link itself, which on macOS is read from the drag pasteboard.

use tauri::AppHandle;

/// The first `http(s)` link in `text`: a plain URL, a URL in a property list
/// (`<string>…</string>`), or one among other words.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // read only on macOS; tested everywhere
fn first_link(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | '(' | ')'))
        .find(|word| {
            let lower = word.to_ascii_lowercase();
            let host = lower
                .strip_prefix("https://")
                .or_else(|| lower.strip_prefix("http://"));
            host.is_some_and(|host| !host.is_empty())
        })
        .map(str::to_owned)
}

/// Picks the link from what the pasteboard holds, best type first.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // read only on macOS; tested everywhere
pub fn pick_link(candidates: &[Option<String>]) -> Option<String> {
    candidates
        .iter()
        .flatten()
        .find_map(|text| first_link(text))
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;

    /// `NSPasteboardNameDrag`. The documented names are given as strings rather than read from
    /// AppKit's extern statics, which would need `unsafe`.
    const DRAG_PASTEBOARD: &str = "Apple CFPasteboard drag";
    /// `NSPasteboardTypeURL`, the legacy `NSURLPboardType`, `NSPasteboardTypeString`.
    const TYPES: [&str; 3] = [
        "public.url",
        "Apple URL pasteboard type",
        "public.utf8-plain-text",
    ];

    /// What the drag pasteboard holds for each type of [`TYPES`].
    pub fn drag_texts() -> Vec<Option<String>> {
        let pasteboard = NSPasteboard::pasteboardWithName(&NSString::from_str(DRAG_PASTEBOARD));
        TYPES
            .iter()
            .map(|kind| {
                pasteboard
                    .stringForType(&NSString::from_str(kind))
                    .map(|text| text.to_string())
            })
            .collect()
    }
}

/// The link being dragged (or just dropped) onto the window, if what is dragged is one.
#[tauri::command]
pub async fn drag_link(app: AppHandle) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // AppKit objects are read on the main thread.
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            let _ = sender.send(macos::drag_texts());
        })
        .ok()?;
        pick_link(&receiver.await.ok()?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Other systems' WebViews don't expose the dragged link to Tauri either; paste works.
        let _ = app;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_url_type() {
        let picked = pick_link(&[
            Some("https://www.youtube.com/watch?v=abc".into()),
            None,
            Some("https://other.example/".into()),
        ]);
        assert_eq!(
            picked.as_deref(),
            Some("https://www.youtube.com/watch?v=abc")
        );
    }

    #[test]
    fn reads_the_legacy_property_list_and_plain_text() {
        let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array><string>https://soundcloud.com/a/b</string><string></string></array></plist>"#;
        assert_eq!(
            pick_link(&[None, Some(plist.into()), None]).as_deref(),
            Some("https://soundcloud.com/a/b")
        );
        assert_eq!(
            pick_link(&[
                None,
                None,
                Some("Listen: http://radio.example:8000/live.pls\n".into())
            ])
            .as_deref(),
            Some("http://radio.example:8000/live.pls")
        );
    }

    #[test]
    fn ignores_files_and_text_without_links() {
        assert_eq!(
            pick_link(&[
                Some("file:///Users/me/Music/a.flac".into()),
                None,
                Some("just some words".into()),
            ]),
            None
        );
        assert_eq!(pick_link(&[None, None, None]), None);
        assert_eq!(pick_link(&[Some("https://".into())]), None);
        assert_eq!(
            pick_link(&[Some("HTTP://a".into())]).as_deref(),
            Some("HTTP://a")
        );
    }
}
