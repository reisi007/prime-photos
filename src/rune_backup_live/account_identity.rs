use super::LiveState;
use crate::config::Config;
use crate::i18n::tr;
use crate::prime_photos::PrimePhotos;
use rune::tok;
use rune::{El, ImageCache, Tx, Waker, col, el, row, text};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const AVATAR_DIAMETER: f32 = 30.0;

#[derive(Default)]
pub(super) struct AccountIdentity {
    pub(super) name: Option<String>,
    pub(super) image: Option<rune::Image>,
    attempts: u8,
    failed: bool,
    in_flight: bool,
    generation: u64,
    mailbox: Arc<Mutex<Mailbox>>,
    wakeup: Option<Waker>,
    image_cache: Option<ImageCache>,
}

#[derive(Default)]
struct Mailbox {
    generation: u64,
    update: Option<FetchUpdate>,
}

struct FetchUpdate {
    name: Option<String>,
    jpeg: Option<Vec<u8>>,
    complete: bool,
    failed: bool,
}

impl AccountIdentity {
    pub(super) fn set_wakeup(&mut self, wakeup: Waker) {
        self.image_cache = Some(ImageCache::new(wakeup.clone()));
        self.wakeup = Some(wakeup);
    }

    pub(super) fn reset_if_signed_out(&mut self) -> bool {
        let has_state = self.attempts > 0
            || self.failed
            || self.in_flight
            || self.name.is_some()
            || self.image.is_some();
        if !has_state {
            return false;
        }

        self.generation = self.generation.wrapping_add(1);
        let mut mailbox = self.mailbox.lock().unwrap();
        mailbox.generation = self.generation;
        mailbox.update = None;
        self.attempts = 0;
        self.failed = false;
        self.in_flight = false;
        self.name = None;
        self.image = None;
        true
    }

    fn start_if_needed(&mut self, config: Config, runtime: Arc<tokio::runtime::Runtime>) {
        let Some(wakeup) = self.wakeup.clone() else {
            return;
        };
        if !should_start_fetch(true, self.attempts, self.failed, self.in_flight) {
            return;
        }

        self.attempts = self.attempts.saturating_add(1);
        self.failed = false;
        self.in_flight = true;
        let generation = self.generation;
        let mailbox = self.mailbox.clone();
        drop(runtime.spawn(async move {
            fetch_account_identity(config, generation, mailbox, wakeup).await;
        }));
    }

    fn apply_update(&mut self) -> bool {
        let update = {
            let mut mailbox = self.mailbox.lock().unwrap();
            if mailbox.generation == self.generation {
                mailbox.update.take()
            } else {
                None
            }
        };
        let Some(update) = update else {
            return false;
        };

        let mut changed = false;
        if let Some(name) = update.name.filter(|name| !name.trim().is_empty()) {
            self.name = Some(name);
            changed = true;
        }
        if let Some(jpeg) = update.jpeg.filter(|bytes| crate::prime_photos::looks_like_image(bytes))
            && let Some(cache) = &self.image_cache
        {
            // The pinned Rune ImageCache decodes on its image worker and wakes
            // this host when the pixels are ready; polling never decodes here.
            self.image = Some(cache.load(jpeg));
            changed = true;
        }
        if update.complete {
            self.in_flight = false;
            self.failed = update.failed;
            // A terminal result wakes the next poll, where a failed first attempt
            // may start its one retry.
            changed = true;
        }
        changed
    }
}

pub(super) fn poll(state: &mut LiveState) -> bool {
    if !state.logged_in() {
        return state.account_identity.reset_if_signed_out();
    }

    let config = state.config.clone();
    let runtime = state.runtime.clone();
    state.account_identity.start_if_needed(config, runtime);
    state.account_identity.apply_update()
}

pub(super) fn sidebar_account(state: &LiveState, theme: tok::Theme) -> El {
    let logged_in = state.logged_in();
    let account_status = if logged_in {
        signed_in_profile(state, theme)
    } else {
        let status = if state.has_pending_session() {
            tr("Sitzung wird wiederhergestellt …")
        } else if state.login.running {
            tr("Anmeldung läuft …")
        } else {
            tr("Nicht angemeldet")
        };
        text(status)
            .key("sidebar-account-status")
            .role("status")
            .font(tok::type_::LABEL)
            .color(theme.text_secondary)
            .wrap()
    };
    let marketplace = col()
        .key("sidebar-marketplace")
        .role("group")
        .label(tr("Marktplatz"))
        .w_full()
        .gap(2.0)
        .child(
            text(tr("Marktplatz"))
                .key("sidebar-marketplace-label")
                .font(tok::type_::LABEL)
                .color(theme.text_secondary)
                .a11y_hidden(),
        )
        .child(
            text(crate::countries::country_label(&state.config.country))
                .key("sidebar-marketplace-value")
                .font(tok::type_::BODY)
                .color(theme.text)
                .wrap(),
        );
    let mut account = col()
        .key("sidebar-account")
        .role("group")
        .label(tr("Konto"))
        .w_full()
        .gap(10.0)
        .child(
            text(tr("Konto"))
                .key("sidebar-account-heading")
                .font(tok::type_::LABEL)
                .color(theme.text_secondary)
                .a11y_hidden(),
        )
        .child(account_status)
        .child(marketplace);
    if logged_in {
        account = account.child(
            super::widgets::action_button(
                "sign-out",
                tr("Abmelden").to_owned(),
                theme,
                theme.control_track,
                theme.text,
                |state: &mut LiveState, tx: &mut Tx| state.logout(tx),
            )
            .w_full(),
        );
    }
    account
}

fn signed_in_profile(state: &LiveState, theme: tok::Theme) -> El {
    let name = state.account_identity.name.as_deref();
    let avatar = state
        .account_identity
        .image
        .as_ref()
        .filter(|image| image.is_ready() && image.error().is_none());
    let avatar = match avatar {
        Some(image) => el()
            .key("account-avatar")
            .w(AVATAR_DIAMETER)
            .h(AVATAR_DIAMETER)
            .radius(AVATAR_DIAMETER / 2.0)
            .clip_rounded(AVATAR_DIAMETER / 2.0)
            .fill(theme.control_track)
            .pixels(image.pixels()),
        None => {
            let initial = name
                .and_then(|value| value.chars().next())
                .map(|character| character.to_uppercase().collect::<String>())
                .unwrap_or_else(|| "•".to_owned());
            el()
                .key("account-avatar-placeholder")
                .w(AVATAR_DIAMETER)
                .h(AVATAR_DIAMETER)
                .radius(AVATAR_DIAMETER / 2.0)
                .clip_rounded(AVATAR_DIAMETER / 2.0)
                .fill(theme.control_track)
                .center()
                .a11y_hidden()
                .child(text(initial).font(tok::type_::BODY).color(theme.text_secondary))
        }
    };
    // The identity arrives a frame or more after the first paint, so the
    // not-yet-loaded state renders the very same two-line structure with a
    // placeholder instead of the name. The name stays on one line (ellipsized
    // when it does not fit) because the block is bottom-anchored: a wrapping
    // name would grow it and shift the avatar, the heading and the divider.
    let details = col()
        .key("account-details")
        .gap(2.0)
        .child(
            text(tr("Angemeldet als:"))
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(
            text(name.map_or_else(|| tr("Wird geladen …").to_owned(), str::to_owned))
                .key("account-name")
                .font(tok::type_::BODY)
                .color(theme.text)
                .truncate(),
        );

    row()
        .key("account-identity")
        .role("group")
        .label(tr("Angemeldetes Konto"))
        .w_full()
        .gap(10.0)
        .center_y()
        .child(avatar)
        .child(details.grow(1.0).min_w(0.0))
}

/// One initial request and at most one retry, and only after a failed result.
fn should_start_fetch(logged_in: bool, attempts: u8, failed: bool, in_flight: bool) -> bool {
    logged_in
        && !in_flight
        && match attempts {
            0 => true,
            1 => failed,
            _ => false,
        }
}

fn attempt_failed(name: Option<&str>, jpeg: Option<&[u8]>) -> bool {
    !name.is_some_and(|value| !value.trim().is_empty())
        && !jpeg.is_some_and(crate::prime_photos::looks_like_image)
}

fn can_publish(current_generation: u64, attempt_generation: u64) -> bool {
    current_generation == attempt_generation
}

fn publish(
    mailbox: &Arc<Mutex<Mailbox>>,
    wakeup: &Waker,
    generation: u64,
    update: FetchUpdate,
) {
    {
        let mut mailbox = mailbox.lock().unwrap();
        if !can_publish(mailbox.generation, generation) {
            return;
        }
        mailbox.update = Some(update);
    }
    wakeup();
}

fn read_cached_avatar(path: &Option<PathBuf>) -> Option<Vec<u8>> {
    path.as_ref()
        .and_then(|path| std::fs::read(path).ok())
        .filter(|bytes| crate::prime_photos::looks_like_image(bytes))
}

fn cache_avatar(path: &Option<PathBuf>, jpeg: &[u8]) {
    let Some(path) = path else {
        return;
    };
    if let Some(directory) = path.parent() {
        let _ = std::fs::create_dir_all(directory);
    }
    if let Err(error) = std::fs::write(path, jpeg) {
        log::warn!("Rune-Kontoidentität: Profilbild-Cache konnte nicht geschrieben werden: {error}");
    }
}

async fn fetch_account_identity(
    config: Config,
    generation: u64,
    mailbox: Arc<Mutex<Mailbox>>,
    wakeup: Waker,
) {
    let photos = PrimePhotos::new(&config, false);
    let (name, avatar_node) = match photos.fetch_account_name().await {
        Ok(Some(pair)) => pair,
        Ok(None) => {
            log::warn!("Rune-Kontoidentität: Kontoeinstellungen enthalten keine verwertbaren Daten.");
            publish(
                &mailbox,
                &wakeup,
                generation,
                FetchUpdate {
                    name: None,
                    jpeg: None,
                    complete: true,
                    failed: attempt_failed(None, None),
                },
            );
            return;
        }
        Err(_) => {
            log::warn!("Rune-Kontoidentität: Kontoeinstellungen konnten nicht abgerufen werden.");
            publish(
                &mailbox,
                &wakeup,
                generation,
                FetchUpdate {
                    name: None,
                    jpeg: None,
                    complete: true,
                    failed: attempt_failed(None, None),
                },
            );
            return;
        }
    };

    let cache_path = crate::prime_photos::avatar_cache_path(&avatar_node);
    let cached = read_cached_avatar(&cache_path);
    publish(
        &mailbox,
        &wakeup,
        generation,
        FetchUpdate {
            name: Some(name.clone()),
            jpeg: cached.clone(),
            complete: false,
            failed: false,
        },
    );

    let jpeg = match photos.fetch_avatar_jpeg(&avatar_node).await {
        Ok(bytes) if crate::prime_photos::looks_like_image(&bytes) => {
            cache_avatar(&cache_path, &bytes);
            Some(bytes)
        }
        Ok(_) | Err(_) => {
            log::warn!("Rune-Kontoidentität: Profilbild konnte nicht abgerufen werden.");
            cached
        }
    };
    let failed = attempt_failed(Some(&name), jpeg.as_deref());
    publish(
        &mailbox,
        &wakeup,
        generation,
        FetchUpdate {
            name: Some(name),
            jpeg,
            complete: true,
            failed,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{AccountIdentity, FetchUpdate, attempt_failed, can_publish, should_start_fetch};

    #[test]
    fn account_fetch_allows_one_retry_only_after_a_failed_attempt() {
        assert!(should_start_fetch(true, 0, false, false));
        assert!(!should_start_fetch(false, 0, false, false));
        assert!(!should_start_fetch(true, 0, false, true));
        assert!(should_start_fetch(true, 1, true, false));
        assert!(!should_start_fetch(true, 1, false, false));
        assert!(!should_start_fetch(true, 2, true, false));
    }

    #[test]
    fn account_attempt_fails_only_when_neither_name_nor_image_is_usable() {
        assert!(attempt_failed(None, None));
        assert!(attempt_failed(Some("  "), None));
        assert!(!attempt_failed(Some("Prime Photos user"), None));
        assert!(!attempt_failed(None, Some(&[0xFF, 0xD8, 0xFF])));
        assert!(attempt_failed(None, Some(b"not an image")));
    }

    #[test]
    fn stale_account_fetch_results_are_ignored_after_session_reset() {
        assert!(can_publish(4, 4));
        assert!(!can_publish(5, 4));
    }

    #[test]
    fn signed_out_reset_clears_identity_and_invalidates_in_flight_fetches() {
        let mut identity = AccountIdentity {
            name: Some("Synthetic account".to_owned()),
            attempts: 1,
            in_flight: true,
            generation: 4,
            ..AccountIdentity::default()
        };
        {
            let mut mailbox = identity.mailbox.lock().unwrap();
            mailbox.generation = 4;
            mailbox.update = Some(FetchUpdate {
                name: Some("Late result".to_owned()),
                jpeg: None,
                complete: true,
                failed: false,
            });
        }

        assert!(identity.reset_if_signed_out());

        assert_eq!(identity.generation, 5);
        assert!(identity.name.is_none());
        assert!(identity.image.is_none());
        assert_eq!(identity.attempts, 0);
        assert!(!identity.in_flight);
        let mailbox = identity.mailbox.lock().unwrap();
        assert_eq!(mailbox.generation, 5);
        assert!(mailbox.update.is_none());
        drop(mailbox);
        assert!(!identity.reset_if_signed_out());
    }
}
