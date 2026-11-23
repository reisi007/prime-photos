//! Small, coherent slice of `LiveState`: the signed-in guard and the two
//! guarded persistence helpers. Split out of the host module purely to stay
//! under the repo's per-file size ceiling; all callsites are unchanged.

use super::LiveState;
use crate::i18n::tr;

impl LiveState {
    pub(super) fn logged_in(&self) -> bool {
        !self.config.session_id.is_empty() && !self.config.cookie_x_acb.is_empty()
    }

    pub(super) fn persist_config_if_allowed(&mut self) {
        if !self.persist_config {
            return;
        }
        if let Err(error) = self.config.save() {
            log::warn!("Rune-Sicherungsroute: Einstellungen konnten nicht gespeichert werden: {error}");
            self.config_error = Some(tr("Die Einstellungen konnten nicht gespeichert werden.").to_owned());
        } else {
            self.config_error = None;
        }
    }

    pub(super) fn persist_store_if_allowed(&mut self) {
        if !self.persist_store {
            return;
        }
        if let Err(error) = self.store.save() {
            log::warn!("Rune-Sicherungsroute: Sicherungsziele konnten nicht gespeichert werden: {error}");
            self.store_error = Some(tr("Sicherungsziele konnten nicht gespeichert werden.").to_owned());
        } else {
            self.store_error = None;
        }
    }
}
