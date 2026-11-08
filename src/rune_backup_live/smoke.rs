use super::widgets::{action_button, section_heading};
use super::{LivePage, HEADER_HEIGHT, MIN_WINDOW_SIZE, WINDOW_SIZE};
use crate::about::LicenseTab;
use crate::i18n::{self, tr, Lang};
use rune::tok;
use rune::{col, row, text, App, AppSpec, Cx, El, ScrollSpec};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SmokeScene {
    Empty,
    Populated,
    #[default]
    Alert,
    About,
    Licenses,
}

#[derive(Clone)]
struct SmokeTarget {
    name: String,
    completed: usize,
    total: usize,
    failed: bool,
}

#[derive(Clone)]
struct SmokeQueueItem {
    name: String,
    status: String,
}

struct SmokeState {
    scene: SmokeScene,
    page: LivePage,
    license_tab: LicenseTab,
    paused: bool,
    targets: Vec<SmokeTarget>,
    queue: Vec<SmokeQueueItem>,
    notice: String,
    retry_count: usize,
}

impl Default for SmokeState {
    fn default() -> Self {
        Self {
            scene: SmokeScene::Alert,
            page: LivePage::Backup,
            license_tab: LicenseTab::Components,
            paused: false,
            targets: alert_targets(),
            queue: populated_queue(true),
            notice: "Nur synthetische Beispieldaten – keine Konten, Ordner oder Uploads."
                .to_owned(),
            retry_count: 0,
        }
    }
}

impl SmokeState {
    fn show_scene(&mut self, scene: SmokeScene) {
        self.scene = scene;
        match scene {
            SmokeScene::Empty => {
                self.page = LivePage::Backup;
                self.targets.clear();
                self.queue.clear();
                self.notice = "Leerer Zustand · ausschließlich synthetische Daten.".to_owned();
            }
            SmokeScene::Populated => {
                self.page = LivePage::Backup;
                self.targets = populated_targets();
                self.queue = populated_queue(false);
                self.notice = "Gefüllter Zustand · ausschließlich synthetische Daten.".to_owned();
            }
            SmokeScene::Alert => {
                *self = Self::default();
                self.notice = "Alert-Zustand · ausschließlich synthetische Daten.".to_owned();
            }
            SmokeScene::About => {
                self.page = LivePage::About;
                self.license_tab = LicenseTab::Components;
            }
            SmokeScene::Licenses => {
                self.page = LivePage::Licenses;
                self.license_tab = LicenseTab::Components;
            }
        }
    }
}

impl super::about::AboutRouteHost for SmokeState {
    fn page(&self) -> LivePage {
        self.page
    }

    fn set_page(&mut self, page: LivePage) {
        self.page = page;
    }

    fn license_tab(&self) -> LicenseTab {
        self.license_tab
    }

    fn set_license_tab(&mut self, tab: LicenseTab) {
        self.license_tab = tab;
    }
}

fn populated_targets() -> Vec<SmokeTarget> {
    vec![
        SmokeTarget {
            name: "Familienfotos".to_owned(),
            completed: 3,
            total: 12,
            failed: false,
        },
        SmokeTarget {
            name: "Reisebilder".to_owned(),
            completed: 2,
            total: 8,
            failed: false,
        },
    ]
}

fn alert_targets() -> Vec<SmokeTarget> {
    let mut targets = populated_targets();
    if let Some(target) = targets.get_mut(1) {
        target.failed = true;
    }
    targets
}

fn populated_queue(with_error: bool) -> Vec<SmokeQueueItem> {
    vec![
        SmokeQueueItem {
            name: "Beispiel_001.jpg".to_owned(),
            status: "Wird übertragen".to_owned(),
        },
        SmokeQueueItem {
            name: "Beispiel_002.jpg".to_owned(),
            status: "Warten".to_owned(),
        },
        SmokeQueueItem {
            name: "Beispiel_003.png".to_owned(),
            status: if with_error { "Fehler" } else { "Warten" }.to_owned(),
        },
    ]
}

const SMOKE_SCENES: &[(&str, fn(&mut SmokeState))] = &[
    ("empty", |state| state.show_scene(SmokeScene::Empty)),
    ("populated", |state| state.show_scene(SmokeScene::Populated)),
    ("alert", |state| state.show_scene(SmokeScene::Alert)),
    ("about", |state| state.show_scene(SmokeScene::About)),
    ("licenses", |state| state.show_scene(SmokeScene::Licenses)),
];

pub(super) fn run_smoke(args: Vec<String>) -> Result<(), String> {
    i18n::set_lang(Lang::De);
    App::new(AppSpec {
        scenes: SMOKE_SCENES,
        min_size: Some(MIN_WINDOW_SIZE),
        ..AppSpec::new(
            "Prime Photos backup smoke",
            WINDOW_SIZE,
            SmokeState::default,
            smoke_view,
        )
    })
    .init(SmokeState::default)
    .args(args)
    .standard_menus()
    .try_run()
}

fn smoke_view(state: &SmokeState, cx: &mut Cx<SmokeState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    let colors = crate::rune_theme::palette(dark);
    cx.foreground(theme.text);
    cx.focus_ring(2.0, 2.0, colors.focus_ring);
    let inset = cx.titlebar_inset();
    let (page_title, page_title_key) = match state.page {
        LivePage::Backup => (
            tr("Sicherungsansicht · visueller Smoke-Test"),
            "smoke-title-backup",
        ),
        LivePage::Settings => (tr("Einstellungen"), "smoke-title-settings"),
        LivePage::About => (tr("Über Prime Photos"), "smoke-title-about"),
        LivePage::Licenses => (tr("Drittanbieterlizenzen"), "smoke-title-licenses"),
    };
    let content = if matches!(state.page, LivePage::About | LivePage::Licenses) {
        super::about::route_content(state, theme, colors)
    } else {
        backup_content(state, theme, colors)
    };
    let header = row()
        .key("smoke-header")
        .w_full()
        .h(HEADER_HEIGHT)
        .pl(inset + 24.0)
        .pr(24.0)
        .center_y()
        .justify_between()
        .fill(theme.card)
        .window_drag()
        .child(
            col()
                .gap(2.0)
                .child(
                    text("Prime Photos")
                        .font(tok::type_::TITLE)
                        .color(theme.text),
                )
                .child(
                    text(page_title)
                        .key(page_title_key)
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .child(
            text("SYNTHETISCHE DATEN")
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        );
    col()
        .key("rune-backup-smoke")
        .role("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(header)
        .child(content)
}

fn backup_content(state: &SmokeState, theme: tok::Theme, colors: crate::rune_theme::Palette) -> El {
    let completed = state
        .targets
        .iter()
        .map(|target| target.completed)
        .sum::<usize>();
    let total = state
        .targets
        .iter()
        .map(|target| target.total)
        .sum::<usize>();
    let mut targets = col()
        .key("smoke-targets")
        .w_full()
        .gap(10.0)
        .child(section_heading(tr("Sicherungsziele"), theme));
    for (index, target) in state.targets.iter().enumerate() {
        targets = targets.child(
            col()
                .key(format!("smoke-target-{index}"))
                .w_full()
                .padding(13.0)
                .gap(6.0)
                .fill(theme.card)
                .radius(10.0)
                .child(
                    row()
                        .w_full()
                        .justify_between()
                        .child(
                            text(target.name.clone())
                                .font(tok::type_::BODY)
                                .color(theme.text),
                        )
                        .child(
                            text(if target.failed {
                                tr("Fehler")
                            } else if state.paused {
                                tr("Angehalten")
                            } else {
                                tr("Upload-Vorgang läuft")
                            })
                            .font(tok::type_::LABEL)
                            .color(if target.failed {
                                colors.alert_foreground
                            } else {
                                theme.text_secondary
                            }),
                        ),
                )
                .child(
                    text(format!(
                        "{} von {} Dateien abgeschlossen",
                        target.completed, target.total
                    ))
                    .font(tok::type_::LABEL)
                    .color(theme.text_secondary),
                ),
        );
    }
    if state.targets.is_empty() {
        targets = targets.child(
            text(tr(
                "Noch keine Sicherungsziele in dieser synthetischen Ansicht.",
            ))
            .key("smoke-empty-targets")
            .font(tok::type_::BODY)
            .color(theme.text_secondary)
            .wrap(),
        );
    }
    let mut queue = col()
        .key("smoke-queue")
        .w_full()
        .gap(6.0)
        .child(section_heading(tr("Warteschlange"), theme));
    for (index, item) in state.queue.iter().enumerate() {
        queue = queue.child(
            row()
                .key(format!("smoke-queue-{index}"))
                .w_full()
                .h(36.0)
                .px(12.0)
                .center_y()
                .justify_between()
                .fill(theme.card)
                .radius(8.0)
                .child(
                    text(item.name.clone())
                        .font(tok::type_::LABEL)
                        .color(theme.text),
                )
                .child(text(item.status.clone()).font(tok::type_::LABEL).color(
                    if item.status == "Fehler" {
                        colors.alert_foreground
                    } else {
                        theme.text_secondary
                    },
                )),
        );
    }
    if state.queue.is_empty() {
        queue = queue.child(
            text(tr("Die synthetische Warteschlange ist leer."))
                .key("smoke-empty-queue")
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        );
    }
    let controls = row()
        .key("smoke-controls")
        .w_full()
        .gap(8.0)
        .child(action_button(
            "smoke-pause",
            if state.paused {
                tr("Fortsetzen")
            } else {
                tr("Anhalten")
            }
            .to_owned(),
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            |state: &mut SmokeState, _| {
                state.paused = !state.paused;
                state.notice = if state.paused {
                    "Lokale Demo pausiert."
                } else {
                    "Lokale Demo fortgesetzt."
                }
                .to_owned();
            },
        ))
        .child(action_button(
            "smoke-advance",
            tr("Demo-Schritt").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut SmokeState, _| {
                if !state.paused {
                    if let Some(target) = state.targets.first_mut() {
                        target.completed = (target.completed + 1).min(target.total);
                    }
                    if let Some(item) = state.queue.first_mut() {
                        item.status = "Abgeschlossen".to_owned();
                    }
                    state.notice = "Synthetischer Fortschritt wurde lokal verschoben.".to_owned();
                }
            },
        ))
        .child(action_button(
            "smoke-add-target",
            tr("Beispielziel hinzufügen").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut SmokeState, _| {
                let number = state.targets.len() + 1;
                state.targets.push(SmokeTarget {
                    name: format!("Demo-Ordner {number}"),
                    completed: 0,
                    total: 5,
                    failed: false,
                });
                state.notice =
                    "Synthetisches Ziel hinzugefügt; keine Datei wurde gelesen.".to_owned();
            },
        ))
        .child(action_button(
            "smoke-scene",
            tr("Ansicht wechseln").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut SmokeState, _| {
                let next = match state.scene {
                    SmokeScene::Alert | SmokeScene::About | SmokeScene::Licenses => {
                        SmokeScene::Empty
                    }
                    SmokeScene::Empty => SmokeScene::Populated,
                    SmokeScene::Populated => SmokeScene::Alert,
                };
                state.show_scene(next);
            },
        ))
        .child(action_button(
            "smoke-retry",
            tr("Fehler erneut versuchen").to_owned(),
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            |state: &mut SmokeState, _| {
                state.retry_count += 1;
                if let Some(target) = state.targets.iter_mut().find(|target| target.failed) {
                    target.failed = false;
                }
                if let Some(item) = state.queue.iter_mut().find(|item| item.status == "Fehler") {
                    item.status = "Warten".to_owned();
                }
                state.notice = format!(
                    "Lokaler Retry-Klick {} – kein Upload gestartet.",
                    state.retry_count
                );
            },
        ));
    col()
        .key("smoke-content")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .w_full()
                .gap(14.0)
                .padding(20.0)
                .child(
                    col()
                        .key("smoke-progress")
                        .w_full()
                        .padding(14.0)
                        .gap(6.0)
                        .fill(theme.card)
                        .radius(12.0)
                        .child(section_heading(tr("Gesamtfortschritt"), theme))
                        .child(
                            text(format!("{completed} von {total} Dateien abgeschlossen"))
                                .font(tok::type_::BODY)
                                .color(theme.text),
                        )
                        .child(
                            text(state.notice.clone())
                                .font(tok::type_::LABEL)
                                .color(theme.text_secondary)
                                .wrap(),
                        ),
                )
                .child(controls)
                .child(targets)
                .child(queue)
                .child(col().grow(1.0)),
        )
}

#[cfg(test)]
mod tests;
