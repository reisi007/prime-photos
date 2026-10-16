# One-time upload (Drag & drop)

A dropped selection is a **one-off upload**: no watch, no watermark, no
permanent backup. The queue is persisted in SQLite so a restart resumes it,
and it is deleted only after a completely finished run.

## How it works

- A dropped selection is a **one-off upload**: no watch, no watermark, no
  permanent backup — only the temporary queue until completion.
- The queue is persisted in the same SQLite database as the MD5 dedup index
  (table `one_time_queue`) and is **deleted only after a completely finished
  run**. This way the row "Einmalige Sicherung" survives a restart and the
  run is resumed (guards like background sync).
- This matches the original, which persists the drop queue
  (`uploads`/`backup`, `schedule_id="ManualUpload"`,
  `../original/SPEC_ONE_TIME_UPLOAD.md` §6).
- **Ausnahme Einmal-Queue (Drag&Drop, User-Entscheidung 2025-10-15):** there
  is **no** folder in `folders.toml` (synthetic index
  `ONE_TIME_IDX = usize::MAX`, `src/app.rs:26`, transient `BackupFolder`).
  The checkbox ("Nicht noch einmal fragen") is effective here **only
  run-wide**; at the next drop the Application Default decides again.
  Deliberately **not** chosen: silently overriding the Application Default
  (it acts globally instead of only for this drop).
  **Unaffected by this:** the 2025-10-14 decision that the drop list
  itself is persisted (SQLite table `one_time_queue`, restored at start,
  deleted only after a completely finished run) — what is **not** persisted
  here is exclusively the **conflict answer**.
- **Ausnahme Einmal-Queue (Drag&Drop, User-Entscheidung 2025-10-15):**
  Conflict handling in the one-time queue uses the **Application Default**
  (`Config.conflict_resolution_default`, `Config.dedup_default`) — the same
  role as for the folder default.

## Decisions

- **One-Time-Queue (Drag&Drop) wird temporär in SQLite gespeichert**
  (Entscheidung 2026-10-14): die abgelegten Dateien landen in derselben
  SQLite, in der der MD5-Dedup-Index liegt, und werden **nach vollständig
  abgeschlossenem Upload wieder gelöscht**. Damit überlebt die Zeile
  „Einmalige Sicherung" einen Neustart und der Lauf wird fortgesetzt (Guards
  wie Hintergrund-Sync). Das entspricht dem Original, das die Drop-Queue
  persistiert (`uploads`/`backup`, `schedule_id="ManualUpload"`,
  `../original/SPEC_ONE_TIME_UPLOAD.md` §6).
  **Verworfen:** die alte Regel „Drop-Queue ist absichtlich transient/nie
  persistiert" (`../original/SPEC_ONE_TIME_UPLOAD.md` §10.3) — sie galt nur,
  bis der User die Parität verlangte.

- **Ausnahme Einmal-Queue (Drag&Drop, User-Entscheidung 2025-10-15):** dort
  gibt es **keinen** Ordner in `folders.toml` (synthetischer Index
  `ONE_TIME_IDX = usize::MAX`, `src/app.rs:26`, transienter `BackupFolder`).
  Das Häkchen wirkt hier **nur run-weit**; beim nächsten Drop entscheidet
  wieder der Application Default. Bewusst **nicht** gewählt: den
  Application Default still überschreiben (greift global statt nur für
  diesen Drop). **Unberührt davon bleibt die 2025-10-14 getroffene Regel, dass
  die Drop-Liste selbst persistiert wird** (SQLite-Tabelle `one_time_queue`,
  beim Start wiederhergestellt, gelöscht erst nach vollständig abgeschlossenem
  Lauf) — nicht persistiert wird hier ausschließlich die
  **Konflikt-Antwort**.

## Original reference

- `../original/SPEC_ONE_TIME_UPLOAD.md` — One-time upload (drag & drop, queue, persistence).
