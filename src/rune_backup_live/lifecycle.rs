/// Consume the Login Item's initial AppKit activation without opening the
/// hidden window. Other activation requests, including later reopen events,
/// remain eligible to show it.
pub(super) fn activation_should_show(
    ignore_initial_login_activation: &mut bool,
    activation_requested: bool,
) -> bool {
    if !activation_requested {
        return false;
    }
    if std::mem::take(ignore_initial_login_activation) {
        return false;
    }
    true
}

pub(super) fn deliver_pending_show_request<T>(
    pending: &mut bool,
    window: Option<&T>,
    open_window: impl FnOnce(&T),
) -> bool {
    if !*pending {
        return false;
    }
    let Some(window) = window else {
        return false;
    };
    *pending = false;
    open_window(window);
    true
}

#[cfg(test)]
mod tests {
    use super::{activation_should_show, deliver_pending_show_request};

    #[test]
    fn initial_login_item_activation_is_consumed_but_later_activation_shows() {
        let mut ignore_initial = true;

        assert!(!activation_should_show(&mut ignore_initial, false));
        assert!(ignore_initial, "an absent activation must not consume the latch");
        assert!(!activation_should_show(&mut ignore_initial, true));
        assert!(!ignore_initial);
        assert!(activation_should_show(&mut ignore_initial, true));
    }

    #[test]
    fn ordinary_activation_shows_without_a_suppression_latch() {
        let mut ignore_initial = false;
        assert!(activation_should_show(&mut ignore_initial, true));
    }

    #[test]
    fn show_request_waits_for_native_window_then_delivers_once() {
        let mut pending = true;
        let mut opens = 0;

        assert!(!deliver_pending_show_request(
            &mut pending,
            None::<&()>,
            |_| opens += 1
        ));
        assert!(pending, "request must remain pending until a window exists");
        assert_eq!(opens, 0);

        assert!(deliver_pending_show_request(
            &mut pending,
            Some(&()),
            |_| opens += 1
        ));
        assert!(!pending, "delivered request must be cleared");
        assert_eq!(opens, 1);
        assert!(!deliver_pending_show_request(
            &mut pending,
            Some(&()),
            |_| opens += 1
        ));
        assert_eq!(opens, 1, "one request must open the window only once");
    }
}
