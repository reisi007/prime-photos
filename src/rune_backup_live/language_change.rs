use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Effect {
    RefreshMenu,
    RefreshTray,
    Persist,
}

pub(super) fn apply_effects(mut apply: impl FnMut(Effect)) {
    apply(Effect::RefreshMenu);
    apply(Effect::RefreshTray);
    apply(Effect::Persist);
}

pub(super) fn set_config_language(config: &mut Config, language: Option<&'static str>) {
    config.language = language.map(str::to_owned);
}

#[cfg(test)]
#[test]
fn language_helper_sets_config_and_emits_refresh_then_persist_effects() {
    let mut config = Config::default();
    set_config_language(&mut config, Some("en"));
    let mut effects = Vec::new();
    apply_effects(|effect| effects.push(effect));

    assert_eq!(config.language.as_deref(), Some("en"));
    assert_eq!(
        effects,
        [Effect::RefreshMenu, Effect::RefreshTray, Effect::Persist]
    );
}
