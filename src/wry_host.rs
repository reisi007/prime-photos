//! Main-thread owner and lifecycle for the application's single embedded Wry child.

use std::cell::RefCell;

use wry::{Rect, WebView};

thread_local! {
    /// Wry/WebKit views are created, resized, and dropped on their native UI thread.
    static CHILD_WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

pub(crate) fn install(webview: WebView) -> Result<(), String> {
    CHILD_WEBVIEW.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return Err("A Prime Photos child WebView is already active".to_owned());
        }
        *slot = Some(webview);
        Ok(())
    })
}

pub(crate) fn set_bounds(bounds: Rect) {
    CHILD_WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.set_bounds(bounds);
        }
    });
}

pub(crate) fn close() {
    CHILD_WEBVIEW.with(|slot| {
        slot.borrow_mut().take();
    });
}

/// Keep Rune's logical-point layout coordinates for Wry's logical child bounds.
/// The rectangle is the laid-out WebView area, so its origin already includes the
/// Rune header and any titlebar inset reserved by that header.
#[cfg(any(all(debug_assertions, feature = "rune-ui-spike"), test))]
pub(crate) fn bounds_from_layout(rect: [f32; 4]) -> [f32; 4] {
    [rect[0], rect[1], rect[2].max(1.0), rect[3].max(1.0)]
}

#[cfg(test)]
mod tests {
    use super::bounds_from_layout;

    #[test]
    fn child_bounds_follow_rune_layout_resize_in_logical_points() {
        assert_eq!(
            bounds_from_layout([0.0, 73.0, 960.0, 607.0]),
            [0.0, 73.0, 960.0, 607.0]
        );
        assert_eq!(
            bounds_from_layout([0.0, 73.0, 1200.0, 727.0]),
            [0.0, 73.0, 1200.0, 727.0]
        );
    }

    #[test]
    fn child_bounds_never_collapse_to_zero_size() {
        assert_eq!(
            bounds_from_layout([4.0, 8.0, 0.0, -2.0]),
            [4.0, 8.0, 1.0, 1.0]
        );
    }
}
