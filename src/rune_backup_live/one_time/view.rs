use super::{accepts_drop_path, LiveState};
use crate::folders::FolderStatus;
use crate::i18n::{tr, tr2};
use crate::platform::app_shared::ONE_TIME_IDX;
use rune::tok;
use rune::{El, col, row, text};

pub(in crate::rune_backup_live) fn drop_zone(
    state: &LiveState,
    theme: tok::Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let (background, foreground, secondary) = if state.drop_hovered {
        (
            colors.primary_fill,
            colors.primary_foreground,
            colors.primary_foreground,
        )
    } else {
        (theme.card, theme.text, theme.text_secondary)
    };
    col()
        .key("one-time-drop-target")
        .w_full()
        .padding(14.0)
        .gap(4.0)
        .fill(background)
        .radius(10.0)
        .child(
            text(tr("Fotos zum Hochladen per Drag & Drop verschieben"))
                .font(tok::type_::LABEL)
                .color(foreground)
                .wrap(),
        )
        .child(
            text(tr("Fotos zum Hochladen ablegen..."))
                .font(tok::type_::LABEL)
                .color(secondary),
        )
        .on_drop_files(|state: &mut LiveState, paths, _| {
            state.start_one_time_upload(paths.to_vec());
        })
        .accepts_files(accepts_drop_path)
        .on_drop_hover(|state: &mut LiveState, over, _| state.drop_hovered = over)
}

pub(in crate::rune_backup_live) fn backup_row(
    state: &LiveState,
    theme: tok::Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let Some(one_time) = state.one_time.as_ref() else {
        return rune::el();
    };
    let (running, paused, done, total) = {
        let upload = state.upload.lock().unwrap();
        let running = upload.active && upload.folder_total.contains_key(&ONE_TIME_IDX);
        (
            running,
            *state.paused.borrow(),
            if running {
                upload.folder_done.get(&ONE_TIME_IDX).copied().unwrap_or(0)
            } else {
                one_time.uploaded_files
            },
            if running {
                upload.folder_total.get(&ONE_TIME_IDX).copied().unwrap_or(0)
            } else {
                one_time.total_files
            },
        )
    };
    let status = if running {
        if paused {
            tr("ANGEHALTEN")
        } else {
            tr("Upload-Vorgang läuft")
        }
    } else {
        match one_time.status {
            FolderStatus::Done => tr("Datensicherung abgeschlossen"),
            FolderStatus::Error => tr("Fehler"),
            FolderStatus::Idle if total > 0 => tr("Warten"),
            _ => tr("Bereit"),
        }
    };
    let progress_status = if running && paused {
        FolderStatus::Paused
    } else if running {
        FolderStatus::Uploading
    } else {
        one_time.status
    };
    let status_color = if one_time.status == FolderStatus::Error && !running {
        colors.alert_foreground
    } else {
        theme.text_secondary
    };
    let progress = crate::upload::folder_progress_text(progress_status, done.min(total), total);
    col()
        .key("one-time-backup-row")
        .w_full()
        .padding(12.0)
        .gap(4.0)
        .fill(theme.card)
        .radius(10.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(
                    text(tr("Einmalige Sicherung"))
                        .font(tok::type_::BODY)
                        .color(theme.text),
                )
                .child(text(status).font(tok::type_::LABEL).color(status_color)),
        )
        .child(
            text(tr2("{} von {} Dateien abgeschlossen", done.min(total), total))
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(text(progress).font(tok::type_::LABEL).color(theme.text_secondary))
        .child(
            text(one_time.path.display().to_string())
                .font(tok::type_::LABEL)
                .color(theme.text_secondary)
                .wrap(),
        )
}
