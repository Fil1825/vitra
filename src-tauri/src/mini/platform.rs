//! What Tauri doesn't offer for the mini player: the state of ⌥/Alt and of the mouse button
//! anywhere on screen, and, on macOS, joining full-screen spaces and fading only the system
//! backdrop behind the page.

use tauri::WebviewWindow;

/// Whether ⌥ (Alt on Windows) is held, whichever app has the keyboard. Linux has no simple
/// way to tell, so click-through is switched off from the settings there.
pub fn option_held() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSEvent, NSEventModifierFlags};
        NSEvent::modifierFlags_class().contains(NSEventModifierFlags::Option)
    }
    #[cfg(target_os = "windows")]
    {
        windows::key_down(windows::VK_MENU)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// Whether the left mouse button is down (a window being dragged); `None` where unknown.
pub fn mouse_down() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSEvent;
        Some(NSEvent::pressedMouseButtons() & 1 != 0)
    }
    #[cfg(target_os = "windows")]
    {
        Some(windows::key_down(windows::VK_LBUTTON))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
mod windows {
    pub use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_LBUTTON, VK_MENU};

    pub fn key_down(key: u16) -> bool {
        // The high bit is set while the key (or button) is down. Reading the state has no
        // preconditions, but every Win32 call is `unsafe` to Rust.
        #[allow(unsafe_code)]
        let state = unsafe {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(i32::from(key))
        };
        state < 0
    }
}

/// Lets the window into other apps' full-screen spaces (with `set_visible_on_all_workspaces`
/// for the normal ones). Runs on the main thread.
#[cfg(target_os = "macos")]
pub fn join_full_screen_spaces(window: &WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
    let Ok(pointer) = window.ns_window() else {
        return;
    };
    // SAFETY: Tauri hands out the window's live `NSWindow`, and we are on the main thread.
    #[allow(unsafe_code)]
    let ns_window: &NSWindow = unsafe { &*pointer.cast::<NSWindow>() };
    let behavior = ns_window.collectionBehavior()
        | NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary;
    ns_window.setCollectionBehavior(behavior);
}

#[cfg(not(target_os = "macos"))]
pub fn join_full_screen_spaces(_window: &WebviewWindow) {}

/// Fades the system backdrop (the vibrancy view window_vibrancy put under the page) without
/// fading the page. Runs on the main thread, after the backdrop is applied.
#[cfg(target_os = "macos")]
pub fn set_backdrop_alpha(window: &WebviewWindow, alpha: f64) {
    use objc2_app_kit::NSView;
    /// `NSVisualEffectViewTagged`'s tag in window-vibrancy 0.8 (`NS_VIEW_TAG_BLUR_VIEW`,
    /// not exported).
    const VIBRANCY_VIEW_TAG: isize = 91_376_254;
    let Ok(pointer) = window.ns_view() else {
        return;
    };
    // SAFETY: Tauri hands out the window's live content view, and we are on the main thread.
    #[allow(unsafe_code)]
    let view: &NSView = unsafe { &*pointer.cast::<NSView>() };
    if let Some(vibrancy) = view.viewWithTag(VIBRANCY_VIEW_TAG) {
        vibrancy.setAlphaValue(alpha);
    }
}

/// Gives the borderless window its rounded shape everywhere, not just in the page. macOS
/// clips the window's content to `radius` and works out its shadow again from what is drawn
/// now: a transparent window takes its shadow (and the thin edge drawn with it) from its
/// pixels when first shown, and before the page has painted, or after a resize, that is a
/// rectangle whose corners and edges show dark around the rounded glass. Windows 11 rounds
/// the window itself (8 px, which the page matches). Runs on the main thread.
pub fn round_window(window: &WebviewWindow, radius: f64) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWindow;
        let Ok(pointer) = window.ns_window() else {
            return;
        };
        // SAFETY: Tauri hands out the window's live `NSWindow`, and we are on the main thread.
        #[allow(unsafe_code)]
        let ns_window: &NSWindow = unsafe { &*pointer.cast::<NSWindow>() };
        if let Some(content) = ns_window.contentView() {
            content.setWantsLayer(true);
            if let Some(layer) = content.layer() {
                layer.setCornerRadius(radius);
                layer.setMasksToBounds(true);
            }
        }
        ns_window.invalidateShadow();
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Graphics::Dwm::{
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
        };
        let _ = radius;
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let preference = DWMWCP_ROUND;
        // SAFETY: a live window handle and a pointer to a value of the size given. Windows 10
        // doesn't know the attribute and returns an error, which changes nothing.
        #[allow(unsafe_code)]
        unsafe {
            DwmSetWindowAttribute(
                hwnd.0,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                std::ptr::from_ref(&preference).cast(),
                size_of_val(&preference) as u32,
            );
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (window, radius);
    }
}
