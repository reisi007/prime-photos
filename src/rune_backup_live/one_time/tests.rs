use super::*;
use super::super::{LivePage, LiveState};
use crate::auth::TokenStore;
use crate::folders::FolderStore;
use crate::login::{Session, WebviewLogin};
use rune::{Env, Runtime};
use std::cell::Cell;
use tokio::sync::watch;

fn classify_from_names(path: &Path) -> DropPathKind {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("folder") => DropPathKind::Directory,
        Some("photo.jpg" | "PHOTO.JPG" | "clip.MP4" | "sidecar.xmp" | "notes.txt") => {
            DropPathKind::File
        }
        _ => DropPathKind::Other,
    }
}

#[test]
fn direct_files_and_recursive_results_use_the_shared_photo_filter() {
    let paths = [
        "photo.jpg",
        "PHOTO.JPG",
        "clip.MP4",
        "sidecar.xmp",
        "notes.txt",
        "folder",
    ]
    .map(PathBuf::from);
    let files = filter_drop_paths(
        &paths,
        classify_from_names,
        |_| {
            [
                PathBuf::from("folder/nested.heic"),
                PathBuf::from("folder/movie.webm"),
                PathBuf::from("folder/sidecar.xmp"),
                PathBuf::from("folder/readme.txt"),
            ]
            .to_vec()
        },
    );
    assert_eq!(
        files,
        [
            PathBuf::from("photo.jpg"),
            PathBuf::from("PHOTO.JPG"),
            PathBuf::from("folder/nested.heic"),
        ]
    );
}

#[test]
fn restored_one_time_queues_are_restricted_to_supported_photos() {
    assert_eq!(
        supported_images(
            ["photo.JPG", "clip.MP4", "sidecar.xmp", "notes.txt"]
                .map(PathBuf::from)
                .to_vec()
        ),
        [PathBuf::from("photo.JPG")]
    );
}

#[test]
fn empty_or_unsupported_drops_are_rejected_without_scanning() {
    for paths in [vec![], vec![PathBuf::from("notes.txt")]] {
        let result = collect_after_guards(
            &paths,
            false,
            true,
            false,
            classify_from_names,
            |_| panic!("a file-only/empty drop must not scan a directory"),
        );
        assert_eq!(result, Err(DropError::Empty));
    }
}

#[test]
fn busy_and_session_guards_precede_any_directory_scan() {
    let paths = [PathBuf::from("folder")];
    let scans = Cell::new(0);
    for (busy, logged_in, invalid, expected) in [
        (true, true, false, DropError::Busy),
        (false, false, false, DropError::SignedOut),
        (false, true, true, DropError::SignedOut),
    ] {
        let result = collect_after_guards(
            &paths,
            busy,
            logged_in,
            invalid,
            classify_from_names,
            |_| {
                scans.set(scans.get() + 1);
                Vec::new()
            },
        );
        assert_eq!(result, Err(expected));
    }
    assert_eq!(scans.get(), 0);
}

#[test]
fn restored_queue_resume_requires_background_sync_session_idle_and_unpaused() {
    assert!(can_resume(false, true, false, true, false));
    for guards in [
        (true, true, false, true, false),
        (false, false, false, true, false),
        (false, true, true, true, false),
        (false, true, false, false, false),
        (false, true, false, true, true),
    ] {
        assert!(!can_resume(guards.0, guards.1, guards.2, guards.3, guards.4));
    }
}

#[test]
fn one_time_plan_publishes_a_synthetic_queue_with_shared_dedup_options() {
    let mut plan = OneTimePlan::from_files(
        [PathBuf::from("first.jpg"), PathBuf::from("second.png")].to_vec(),
        true,
        ConflictResolution::Replace,
        |path| Some(if path.extension().and_then(|ext| ext.to_str()) == Some("png") {
            20
        } else {
            10
        }),
    );
    let shared = Arc::new(Mutex::new(UploadShared::default()));
    publish_state(&mut plan, &shared, false);
    let state = shared.lock().unwrap();
    assert!(state.active);
    assert_eq!(state.total, 2);
    assert_eq!(state.bytes_total, 30);
    assert_eq!(state.folder_total.get(&ONE_TIME_IDX), Some(&2));
    assert_eq!(state.queue.len(), 2);
    assert!(state.queue.iter().all(|entry| entry.folder == ONE_TIME_IDX));
    assert_eq!(
        state.folder_opts.get(&ONE_TIME_IDX),
        Some(&(true, ConflictResolution::Replace))
    );
}

#[test]
fn transient_one_time_target_uses_the_photo_only_content_filter() {
    let target = transient_target(
        &Config::default(),
        PathBuf::from("/synthetic/photos"),
        1,
    );
    assert_eq!(target.content_filter, ContentFilter::Photos);
}

#[derive(Default)]
struct NoopTokenStore;

impl TokenStore for NoopTokenStore {
    fn save(&self, _: &str) -> Result<(), String> {
        Ok(())
    }

    fn load(&self) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn clear(&self) -> Result<(), String> {
        Ok(())
    }
}

fn signed_out_state() -> LiveState {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let (paused, _) = watch::channel(false);
    LiveState {
        config: Config::default(),
        page: LivePage::Backup,
        license_tab: crate::about::LicenseTab::Components,
        settings_tab: crate::rune_settings::SettingsTab::General,
        autostart: crate::platform::app_shared::Autostart::unavailable_for_test(),
        autostart_enabled: false,
        settings_limit_edit: rune::TextEdit::new("0"),
        settings_album_edit: rune::TextEdit::new(""),
        settings_slider_origin: 1,
        store: FolderStore::default(),
        config_error: None,
        store_error: None,
        persist_config: false,
        persist_store: false,
        scan_tx: None,
        upload: Arc::new(Mutex::new(UploadShared::default())),
        paused,
        runtime,
        login: WebviewLogin::new(),
        session: Arc::new(Mutex::new(Session::default())),
        token_store: Arc::new(NoopTokenStore),
        session_wakeup: None,
        account_identity: super::super::account_identity::AccountIdentity::default(),
        native_window: None,
        app_handle: None,
        lifecycle_route: super::super::lifecycle_route::LifecycleRoute::default(),
        tray_route: super::super::tray_route::TrayRoute::default(),
        pending_show_request: false,
        child_bounds: None,
        form: None,
        status: None,
        was_active: false,
        one_time: None,
        one_time_resume: false,
        drop_hovered: false,
    }
}

#[test]
fn production_backup_tree_highlights_and_guards_a_file_drop_without_side_effects() {
    let mut runtime = Runtime::new(
        signed_out_state(),
        super::super::view::live_view,
        (1180.0, 820.0),
        Env {
            system_dark: false,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    let point = runtime
        .find("one-time-drop-target")
        .expect("production Backup has a Rune drop target");
    let photo = PathBuf::from("synthetic-photo.jpg");
    runtime.drag_files(vec![photo.clone()], Some(point));
    runtime.frame(1.0 / 60.0);
    assert!(runtime.state.drop_hovered);
    assert_eq!(runtime.drop_hovered().as_deref(), Some("one-time-drop-target"));

    runtime.drop_files(vec![photo], Some(point));
    runtime.frame(2.0 / 60.0);
    assert!(!runtime.state.drop_hovered);
    assert_eq!(
        runtime.state.status.as_deref(),
        Some(tr("Bitte zuerst anmelden, bevor ein Upload gestartet wird."))
    );
    assert!(runtime.state.one_time.is_none());
    let upload = runtime.state.upload.lock().unwrap();
    assert!(!upload.active);
    assert!(upload.queue.is_empty());
}

#[test]
fn production_backup_tree_renders_the_restored_transient_one_time_row() {
    let mut state = signed_out_state();
    state.one_time = Some(BackupFolder {
        path: PathBuf::from("/synthetic/photos"),
        name: "Einmalige Sicherung".to_owned(),
        enabled: true,
        dedup: true,
        conflict_resolution: ConflictResolution::Ignore,
        content_filter: ContentFilter::Photos,
        total_files: 2,
        uploaded_files: 0,
        last_updated: String::new(),
        status: FolderStatus::Idle,
    });
    let mut runtime = Runtime::new(
        state,
        super::super::view::live_view,
        (1180.0, 820.0),
        Env {
            system_dark: false,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    assert!(runtime.find("one-time-drop-target").is_some());
    assert!(runtime.find("one-time-backup-row").is_some());
}
