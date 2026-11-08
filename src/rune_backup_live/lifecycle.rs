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
    use super::deliver_pending_show_request;

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
