use super::{LivePage, MIN_WINDOW_SIZE, WINDOW_SIZE};
use crate::about::LicenseTab;
use crate::i18n::{self, Lang};
use rune::{App, AppSpec};

mod scenes;
mod view;
#[cfg(test)]
mod tests;

use view::smoke_view;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmokeScene {
    Mixed,
    Uploading,
    Paused,
    AllDone,
    Failures,
    Empty,
    ManyFolders,
    About,
    Licenses,
}

impl SmokeScene {
    fn id(self) -> &'static str {
        match self {
            Self::Mixed => "mixed",
            Self::Uploading => "uploading",
            Self::Paused => "paused",
            Self::AllDone => "all-done",
            Self::Failures => "failures",
            Self::Empty => "empty",
            Self::ManyFolders => "many-folders",
            Self::About => "about",
            Self::Licenses => "licenses",
        }
    }

    fn next_backup(self) -> Self {
        match self {
            Self::Mixed => Self::Uploading,
            Self::Uploading => Self::Paused,
            Self::Paused => Self::AllDone,
            Self::AllDone => Self::Failures,
            Self::Failures => Self::Empty,
            Self::Empty => Self::ManyFolders,
            Self::ManyFolders | Self::About | Self::Licenses => Self::Mixed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetStatus {
    Uploading,
    Done,
    Waiting,
    Error,
    NotScanned,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueueStatus {
    Done,
    Uploading,
    Waiting,
    Failed,
}

#[derive(Clone)]
struct SmokeTarget {
    name: String,
    completed: usize,
    total: usize,
    status: TargetStatus,
}

#[derive(Clone)]
struct SmokeQueueItem {
    name: String,
    folder: String,
    status: QueueStatus,
    percent: u8,
}

struct SmokeState {
    scene: SmokeScene,
    page: LivePage,
    license_tab: LicenseTab,
    paused: bool,
    targets: Vec<SmokeTarget>,
    queue: Vec<SmokeQueueItem>,
    progress_done: usize,
    progress_total: usize,
    notice: String,
    retry_count: usize,
}

impl Default for SmokeState {
    fn default() -> Self {
        scenes::state_for(SmokeScene::Mixed)
    }
}

impl SmokeState {
    fn show_scene(&mut self, scene: SmokeScene) {
        *self = scenes::state_for(scene);
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

const SMOKE_SCENES: &[(&str, fn(&mut SmokeState))] = &[
    ("mixed", |state| state.show_scene(SmokeScene::Mixed)),
    ("uploading", |state| state.show_scene(SmokeScene::Uploading)),
    ("paused", |state| state.show_scene(SmokeScene::Paused)),
    ("all-done", |state| state.show_scene(SmokeScene::AllDone)),
    ("failures", |state| state.show_scene(SmokeScene::Failures)),
    ("empty", |state| state.show_scene(SmokeScene::Empty)),
    ("many-folders", |state| state.show_scene(SmokeScene::ManyFolders)),
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
