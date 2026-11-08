use super::{
    apply_target_options, create_target_form, edit_target_form, select_target_photos,
};
use crate::folders::{BackupFolder, ConflictResolution, ContentFilter};
use std::path::PathBuf;

#[test]
fn create_form_defaults_to_photos_and_saves_that_filter() {
    let form = create_target_form(
        PathBuf::from("/photos"),
        true,
        ConflictResolution::Ask,
    );
    assert_eq!(form.content_filter, ContentFilter::Photos);

    let mut folder = BackupFolder::new(form.path.clone());
    apply_target_options(
        &mut folder,
        form.dedup,
        form.conflict,
        form.content_filter,
    );
    assert_eq!(folder.content_filter, ContentFilter::Photos);
}

#[test]
fn edit_form_round_trips_each_persisted_filter() {
    for persisted in [
        ContentFilter::Photos,
        ContentFilter::Videos,
        ContentFilter::PhotosAndVideo,
    ] {
        let mut folder = BackupFolder::new(PathBuf::from("/photos"));
        folder.content_filter = persisted;

        let form = edit_target_form(&folder);
        assert_eq!(form.content_filter, persisted);
        apply_target_options(
            &mut folder,
            form.dedup,
            form.conflict,
            form.content_filter,
        );
        assert_eq!(folder.content_filter, persisted);
    }
}

#[test]
fn photo_only_toggle_preserves_the_persisted_video_checkbox_selection() {
    for (persisted, photos_off, photos_on) in [
        (ContentFilter::Photos, ContentFilter::Photos, ContentFilter::Photos),
        (ContentFilter::Videos, ContentFilter::Videos, ContentFilter::Photos),
        (
            ContentFilter::PhotosAndVideo,
            ContentFilter::Videos,
            ContentFilter::PhotosAndVideo,
        ),
    ] {
        assert_eq!(select_target_photos(persisted, false), photos_off);
        assert_eq!(select_target_photos(persisted, true), photos_on);
    }
}
