# Prime Photos — Build & Run pro Plattform (macOS / Windows / Linux)

Sachliche Anleitung, wie dieses Projekt gebaut und gestartet wird. Alles zu den Abhängigkeiten und
Pfaden ist **aus dem Repo** abgeleitet (`Cargo.toml`, `Cargo.lock`, `src/`); die System-Voraussetzungen
zusätzlich aus der offiziellen Crate-Dokumentation (docs.rs / Repo-README der jeweiligen Crate). Was
nicht belegbar war, steht in §7 ausdrücklich als „nicht verifiziert" und wird hier nicht behauptet.

---

## 1. Testgrenze (wichtig)

**Gebaut *und* laufen getestet ist ausschließlich macOS (Apple Silicon, `aarch64-apple-darwin`).**
Windows und Linux sind erklärte, gleichwertige Ziele (`docs/features/platform.md`) und werden im
Release-Workflow **kompiliert** (Windows x64 auf `windows-latest`, Linux x64 auf `ubuntu-latest`;
beide Beine waren bisher bei jedem Release grün) — **laufgetestet** sind sie damit nicht. Ein
Cross-Compile-Check auf dem Referenzrechner (`cargo check --target …`) ist weiterhin **nicht
durchgeführt**: dort sind nur die Rust-Targets `aarch64-apple-darwin` und `wasm32-unknown-unknown`
installiert (`rustup target list --installed`), ein Windows- oder Linux-Target **nicht**. Die
Abschnitte 4 und 5 sind daher Anleitungen auf Basis der Crate-Vorgaben und des CI-Builds, kein
Erfahrungsbericht aus eigenen Läufen.

### Warnungs-Baseline: 0 (User-Entscheidung 2026-10-15)

**Erzwungen (User-Entscheidung 2026-10-15, „cargo warnings sollen als error behandelt
werden"):** `[lints.rust] warnings = "deny"` in `Cargo.toml` — ab sofort ist **jede** Warnung ein
**harter Build-Fehler**, kein Hinweis mehr. `[lints]` greift nur für **unser** Paket (`src/`,
`build.rs`, Tests), **nicht** für Dependencies (die laufen ohnehin mit `cap-lints`); Rust ≥ 1.74
ist erfüllt (edition 2024).

**Einzige Ausnahme — und sie ist begründet:** `folders::supported_extensions()`
(`src/folders.rs:29-37`) ist bewusst **nicht** verdrahtet und trägt ein punktuelles
`#[allow(dead_code)]`; das überschreibt das paketweite `deny` (empirisch belegt in Verify Runde 1
zu `c019528`). Es ist der vorbereitete Haken für einen späteren Datei-Picker und wird von **keiner**
Codestelle aufgerufen. Neue Karteileichen bleiben verboten: nur dieser eine, dokumentierte Haken
ist erlaubt.

**Verworfen:** `.cargo/config.toml` mit `rustflags = ["-Dwarnings"]` (würde **auch** Deps hart
nehmen, deren Warnungen wir nicht beheben können — ein fremder Patch würde unseren Build rot
machen) und die Erzwingung **nur** in der CI (es gibt bislang **kein** Workflow, und lokal bliebe
es weich statt hart).

### `cargo fmt --check` ist kein Gate (Befund 2026-10-15)

Das Repo ist **nicht** fmt-clean als Baseline — `cargo fmt --check` meldet Abweichungen in
**unangetasteten** Dateien (`build.rs`, `src/app.rs`: Import-Reihenfolge und Zeilenumbrüche). Ein
fmt-Diff ist damit **kein** Regressionssignal und darf einen Task nicht blockieren;
Regressionssignal ist allein die **Warnungs-Baseline 0** (oben).

**Stand 2026-10-16:** `cargo fmt --check` ist weiterhin **nicht** grün (102 Hunks in 19
Dateien). Die **einmalige Formatierung** des Altbestands ist **geplant** (erster Punkt im Ledger
`AGENTS.todo.md`); **danach** gilt `cargo fmt --check` als Gate (User-Entscheidung 2026-10-16).

---

## 2. Toolchain & Projektform

| Punkt | Wert | Beleg |
|---|---|---|
| Paket / Binary | `prime-photos` | `Cargo.toml` `[package] name`; kein `[[bin]]`-Block ⇒ Binary heißt wie das Paket |
| Edition | `2024` ⇒ rustc **≥ 1.85** | `Cargo.toml` `edition = "2024"` |
| Rust-Version auf der Test-Maschine | rustc / cargo **1.98.0** | `rustc --version`, `cargo --version` |
| Festgeschriebene Toolchain | **keine** | weder `rust-toolchain.toml` noch `rust-toolchain` in der Repo-Wurzel |
| `build.rs` | **nicht vorhanden** | Repo-Wurzel |
| CLI-Flags | **keine** | `clap` ist in `Cargo.toml` deklariert, in `src/` jedoch **nicht** verwendet |
| Log-Ausgabe | derzeit **keine** | `env_logger` ist deklariert, wird aber nirgends initialisiert (kein `env_logger::init()` in `src/`) ⇒ `RUST_LOG` hat aktuell keine Wirkung |

Rust installieren: <https://rustup.rs> (`rustup-init`, bzw. auf Windows mit dem MSVC-Target, siehe §4).

### 2.1 Die real genutzten Bibliotheken (Kern)

Version links = Anforderung aus `Cargo.toml`, in Klammern = aufgelöste Version aus `Cargo.lock`.

| Crate | Version | Rolle | System-Relevanz |
|---|---|---|---|
| `eframe` / `egui` | 0.31 (0.31.1) | GUI (Fenster, Rendering, Widgets) | Linux: X11/Wayland-, GL- und xkbcommon-Bibliotheken |
| `wry` | 0.57.0 (0.57.0) | eingebettetes Login-WebView | Linux: WebKitGTK 4.1 + GTK3; Windows: WebView2; macOS: systemeigenes WebKit |
| `tray-icon` | 0.26 (0.26.1) | Tray-/Menüleisten-Icon (`src/tray.rs`, `mod tray` in `src/main.rs`) | Linux: GTK3 + AppIndicator-Bibliothek (siehe §5) |
| `notify-rust` | 4.18 (4.18.2) | Desktop-Benachrichtigungen (`src/notify.rs`) | macOS: `mac-notification-sys`; Windows: `tauri-winrt-notification`; Linux: D-Bus über `zbus` (reines Rust) |
| `rusqlite` | 0.40.2 (0.40.2), **Feature `bundled`** | persistenter Dedup-Index (`src/dedup.rs`) | **C-Compiler nötig**: `bundled` übersetzt SQLite aus mitgelieferten Quellen |
| `reqwest` | 0.12.15 (0.12.28), Feature `json` | Upload-API | TLS über `native-tls` (siehe §2.2) |
| `dirs` | 6.0.0 (6.0.0) | Konfig-/Datenpfad | siehe §6 |
| `dispatch2`, `objc2`, `objc2-app-kit`, `objc2-foundation` | 0.3 / 0.6 / 0.3 / 0.3 | macOS-Sonderteile (App-Menü, Main-Queue-Probe) | **nur macOS** — `Cargo.toml`, `[target.'cfg(target_os = "macos")'.dependencies]` |

### 2.2 TLS: `native-tls`, nicht rustls (wichtig für Linux)

`reqwest` wird ohne `default-features = false` benutzt, d. h. mit dem Default-Feature `default-tls`.
Das ist **`native-tls`** — belegt aus dem aufgelösten Abhängigkeitsgraphen (`cargo tree`), nicht nur
aus dem Lockfile:

```
native-tls v0.2.18
├── hyper-tls v0.6.0 ─── reqwest v0.12.28 ─── prime-photos v0.1.0
├── tokio-native-tls v0.3.1
└── reqwest v0.12.28
```

Die Plattform-Backends von `native-tls` 0.2.18 im Lockfile: `openssl`/`openssl-sys` (Linux/BSD),
`schannel` (Windows), `security-framework` (macOS).

`rustls`, `hyper-rustls` und `tokio-rustls` **stehen zwar im `Cargo.lock`** (als optionale Abhängigkeiten
von `reqwest` erfasst), sind aber **nicht im tatsächlichen Graphen**:
`cargo tree -i rustls` / `-i hyper-rustls` / `-i tokio-rustls` melden „nothing to print".

**Konsequenz für Linux:** Es wird die **System-OpenSSL** gelinkt, nicht eine mitgelieferte Kopie — das
Paket `openssl-src` (Vendoring) fehlt im `Cargo.lock` vollständig. Benötigt wird daher das
OpenSSL-Entwicklungspaket (`libssl-dev` / `openssl-devel`, siehe §5).

---

## 3. macOS (das getestete Ziel)

### Voraussetzungen

- Rust-Toolchain (Edition 2024 ⇒ rustc ≥ 1.85).
- **Xcode Command Line Tools** — liefert den C-Compiler und das macOS-SDK. Nötig, weil `rusqlite`
  mit `bundled` SQLite aus C-Quellen übersetzt (`libsqlite3-sys` nutzt dafür das `cc`-Crate).
  Üblicher Weg: `xcode-select --install`.
  *Beleglage:* Die C-Compile-Anforderung ist über die rusqlite-Doku belegt („If you use the `bundled`…
  features, `libsqlite3-sys` will use the `cc` crate to compile SQLite … from source"); das konkrete
  Kommando `xcode-select --install` steht **nicht** in der Repo-Doku (Standardweg, siehe §7).
- Weitere Frameworks sind **nicht** zu installieren: `wry` nutzt auf macOS das systemeigene WebKit
  ("WebKit is native on macOS so everything should be fine" — wry-Doku).

### Bauen und starten

```sh
cargo build              # Debug-Binary: target/debug/prime-photos
cargo run                # baut (falls nötig) und startet
cargo build --release    # Release-Binary: target/release/prime-photos
```

Es gibt keine CLI-Flags (§2). Das Ergebnis ist im Dev-Build ein **nacktes, unsigniertes Binary** —
kein `.app`-Bundle:

- **Paketierung (Bundle + (selbst-)Signatur) ist PORT-6 und noch offen** (`AGENTS.todo.md`).
- `AGENTS.md` hält fest, dass auf macOS eine (selbst-)signierte Codesignatur nötig ist, „damit
  Benachrichtigungen/Bundle-Identifier sauber funktionieren". Im Dev-Build ist die Signatur **nicht**
  gegeben; ob Benachrichtigungen ohne Bundle/ Signatur erscheinen, ist hier **nicht geprüft**.

macOS-spezifisch (und daher für die anderen Plattformen im Code gegated bzw. No-op):

| Datei / Stelle | Verhalten |
|---|---|
| `src/app_menu.rs` | natives App-Menü, `#[cfg(target_os = "macos")]` (⌘X/⌘C/⌘V im WebView) |
| `src/login.rs` (`Zeilen 15`, `238`) | Cookie-Probe über die Main-Queue nur auf macOS |
| `src/tray.rs` (`Zeile 78`) | `with_icon_templated(icon)` nur auf macOS, sonst `with_icon(icon)` |

---

## 4. Windows (Ziel, hier nicht laufgetestet)

### Voraussetzungen

- **Rust mit MSVC-Target** (`x86_64-pc-windows-msvc`, der rustup-Default auf Windows).
- **Visual Studio Build Tools** mit der Workload „Desktop development with C++", d. h. MSVC-Buildtools
  (`cl.exe`) und ein Windows-SDK. Die rustup-Doku begründet das damit, dass Rust einen Linker, die
  Bibliotheken und die Windows-API-Importbibliotheken braucht; konkret dokumentiert sind die
  Einzelkomponenten „MSVC v143 – VS 2022 C++ x64/x86 build tools (Latest)" und „Windows 11 SDK".
  Für dieses Projekt wird der C-Compiler **zusätzlich** gebraucht, weil `rusqlite` mit `bundled`
  SQLite aus C-Quellen übersetzt (§2.1). Der MSVC-Weg ist auch der von rusqlite empfohlene
  („`bundled` … is a good option for cases where linking to SQLite is complicated, such as Windows").
- **WebView2-Runtime** (Microsoft Edge Chromium) für `wry`. Die wry-Doku nennt Windows 7, 8, 10 und 11
  als unterstützt; WebView2 ist auf aktuellen Windows-10/11-Installationen üblicherweise vorhanden,
  Win7/8 brauchen die Runtime separat. Da es **keinen Installer im Repo** gibt, muss die Runtime auf
  dem Zielsystem vorhanden sein.
- **Kein OpenSSL nötig**: `native-tls` benutzt auf Windows `schannel` (System-TLS).

### Bauen und starten

```cmd
cargo build
cargo run
cargo build --release    :: Release-Binary: target\release\prime-photos.exe
```

---

## 5. Linux (Ziel, hier nicht laufgetestet)

Abgeleitet aus den System-Dep-Listen der drei grafischen Crates, die dieses Projekt einbindet
(`wry` = WebView, `tray-icon` = Tray/AppIndicator, `eframe` = Fenster/Rendering) plus dem
OpenSSL-Bedarf aus §2.2.

### Voraussetzungen — Paketliste

| Baustein | Crate / Feature | Debian / Ubuntu | Fedora | Arch |
|---|---|---|---|---|
| WebView | `wry` (WebKitGTK 4.1 + GTK3) | `libwebkit2gtk-4.1-dev` | `webkit2gtk4.1-devel`, `gtk3-devel` | `webkit2gtk-4.1` |
| Tray-Icon | `tray-icon`, Default-Feature `libappindicator` (GTK3-AppIndicator) | `libgtk-3-dev`, `libappindicator3-dev` *oder* `libayatana-appindicator3-dev` | *(vom Crate nicht dokumentiert, siehe §7)* | `gtk3`, `libappindicator-gtk3` *oder* `libayatana-appindicator` |
| Fenster / Rendering | `eframe` (Default-Features `glow`, `x11`, `wayland`) | `libxcb-render0-dev`, `libxcb-shape0-dev`, `libxcb-xfixes0-dev`, `libxkbcommon-dev` | `libxkbcommon-devel`, `libxcb-devel`, `fontconfig-devel` | *(nicht dokumentiert, siehe §7)* |
| TLS | `reqwest` → `native-tls` → `openssl`/`openssl-sys` | `libssl-dev` | `openssl-devel` | *(nicht dokumentiert)* |
| Build-Werkzeuge | `-sys`-Crates nutzen `pkg-config` (im Lockfile: `pkg-config`, `system-deps`) | `pkg-config` | `pkg-config` | `pkg-config` |
| C-Compiler | `rusqlite` `bundled` (SQLite aus Quellen) | `build-essential` (bzw. `gcc`/`clang`) | `gcc`/`clang` | `base-devel` |

Konkrete Befehle (so, wie die Crates sie dokumentieren, zusammengesetzt):

```sh
# Debian / Ubuntu
sudo apt install pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev \
  build-essential

# Fedora
sudo dnf install pkg-config webkit2gtk4.1-devel gtk3-devel libxkbcommon-devel libxcb-devel \
  openssl-devel fontconfig-devel gcc

# Arch / Manjaro
sudo pacman -S pkg-config webkit2gtk-4.1 gtk3 libappindicator-gtk3 base-devel
```

Einordnung (belegt, nicht geraten):

- **WebKitGTK 4.1 + GTK3** ist die von `wry` vorgegebene Linux-Basis (Doku: „WebKitGTK is used to
  provide webviews on Linux which requires GTK", Paketzeilen für Arch/Debian/Fedora wie oben).
- **AppIndicator**, nicht `ksni`: `tray-icon` hat das Default-Feature `libappindicator` aktiviert,
  und der aufgelöste Graph enthält `libappindicator 0.9.0` + `libappindicator-sys`, aber **kein**
  `ksni` (`cargo tree -i ksni` ⇒ „did not match any packages"). Die `tray-icon`-Doku nennt für
  Linux „GTK 3 and `libappindicator` or `libayatana-appindicator`". Wäre das Feature `ksni` aktiv,
  bräuchte es diese Systembibliotheken nicht — es ist hier aber **nicht** aktiv.
- **glow (GL), nicht wgpu**: `cargo tree` zeigt `egui_glow` und `glow` als Teil von `eframe`,
  `wgpu`/`egui-wgpu` stehen nur im `Cargo.lock`, sind aber nicht im Graphen. `eframe` 0.31 hat die
  Features `x11` und `wayland` per Default an — daher die xcb/xkbcommon-Pakete.
- **OpenSSL, nicht rustls**: siehe §2.2 — es wird die Systembibliothek gelinkt.
- **Benachrichtigungen zur Laufzeit**: `notify-rust` spricht auf Linux über D-Bus (`zbus`, reines
  Rust) mit einem Notification-Daemon. Ein Dev-Paket ist dafür nicht nötig; zur **Laufzeit** muss
  aber ein Notification-Server (`org.freedesktop.Notifications`) laufen. Das ist eine Laufzeit-,
  keine Build-Anforderung und hier **nicht geprüft**.

### Bauen und starten

```sh
cargo build
cargo run
cargo build --release    # Release-Binary: target/release/prime-photos
```

Hinweis zur Plattformbindung: Das Login-WebView ist ein `wry`-**Kind-View**; auf Linux ist `wry` an
GTK gekoppelt. Der Code reicht das Fenster über `raw-window-handle` durch (`AGENTS.md`), ein
zusätzlicher GTK-Integrationstest ist hier aber nicht durchgeführt (§7).

---

## 6. Konfigurationspfade

`src/config.rs` (`Config::get_config_path`) legt den Pfad als
`dirs::config_dir()/prime_photos/config.toml` fest. Denselben Basisordner benutzen
`src/folders.rs` (`store_path()` → `…/prime_photos/folders.toml`) und
`src/dedup.rs` (`db_path()` → `…/prime_photos/dedup.sqlite3`). Dort liegt unter Unix auch
`…/prime_photos/instance.sock` — der Unix-Socket des Einzelinstanz-Schutzes
(`src/single_instance.rs`, `acquire()`); auf Windows benutzt derselbe Schutz statt einer Datei
einen Loopback-TCP-Port.

`dirs` 6.0.0 dokumentiert `config_dir()` so:

| Plattform | Basispfad (`dirs::config_dir()`) | Datei `config.toml` |
|---|---|---|
| macOS | `$HOME/Library/Application Support` | `~/Library/Application Support/prime_photos/config.toml` |
| Windows | `{FOLDERID_RoamingAppData}` | `%APPDATA%\prime_photos\config.toml` |
| Linux | `$XDG_CONFIG_HOME` oder `$HOME/.config` | `~/.config/prime_photos/config.toml` |

Fällt `config_dir()` aus, weichen `folders.rs` und `dedup.rs` auf `std::env::temp_dir()` aus
(`config.rs` liefert in dem Fall einen Fehler). Das Repo enthält außerdem `config_setup.py` — ein
Python-3-Hilfsskript, das dieselben Pfade berechnet und beim ersten Befüllen der `config.toml` hilft.
Es ist **nicht** Teil des Builds.

---

## 7. Nicht verifiziert / offen

- **Kein Windows- und kein Linux-Lauf getestet.** Aussagen in §4/§5 stützen sich auf die
  Crate-Dokumentation, den aufgelösten Abhängigkeitsgraphen und den CI-Build (Release-Workflow, §1),
  nicht auf einen eigenen Lauf auf diesen Systemen.
- **Cross-Compile-Check** (`cargo check --target x86_64-pc-windows-msvc` bzw. `…-unknown-linux-gnu`)
  ist **nicht durchgeführt**; die Targets sind auf dem Referenzrechner nicht installiert.
- `xcode-select --install` als macOS-Voraussetzung: der C-Compiler-Bedarf ist über rusqlite belegt,
  das konkrete Kommando steht **nicht** in der Repo-Doku.
- Fedora-Paketnamen für den AppIndicator-Backend sind vom `tray-icon`-README **nicht** dokumentiert
  (dort nur Arch und Debian/Ubuntu) ⇒ bewusst leer gelassen, nicht geraten. Ebenso fehlen
  Fedora/Arch-Angaben für die `eframe`-Renderer-Bibliotheken (die Fedora-Zeile in §5 stammt aus dem
  `eframe_template`-README, das nur Debian und Fedora listet).
- Ob die WebView2-Runtime auf realen Windows-10/11-Systemen tatsächlich immer vorinstalliert ist,
  ist **nicht geprüft**.
- Ob macOS-Benachrichtigungen im unsignierten Dev-Build erscheinen, ist **nicht geprüft** (hängt laut
  `AGENTS.md` mit Signatur/Bundle zusammen; Bundling ist PORT-6, offen).
- Der „Machen-Tray-Icon-und-Notifications-auf-Windows/Linux"-Lauf ist offen (`PORT-2` fertig,
  `PORT-3` Tray, `PORT-4` Autostart, `PORT-5` Hintergrund-Sync — laut `AGENTS.todo.md`).
- `src/` wird aktuell in Module aufgeteilt (Refactor ohne Verhaltensänderung, `AGENTS.md`,
  „Modul-Struktur"); die hier genannten Datei- und Zeilennummern können dadurch wandern.

---

## 8. Quellen

Repo (gelesen): `Cargo.toml`, `Cargo.lock`, `src/config.rs`, `src/folders.rs`, `src/dedup.rs`,
`src/tray.rs`, `src/main.rs` (`mod`-Deklarationen), `AGENTS.md`, `AGENTS.todo.md`, `config_setup.py`.

Aufgelöster Graph (`cargo tree`, lokal): `native-tls`/`openssl`/`schannel`/`security-framework`,
`hyper-tls`, `tokio-native-tls`, `egui_glow`/`glow`/`glutin`, `libappindicator`; negativ:
`rustls`, `hyper-rustls`, `tokio-rustls`, `wgpu`, `egui-wgpu`, `ksni`.

Crate-Doku / Repo-README:

- `wry` 0.57.0 — <https://docs.rs/wry/0.57.0/wry/> (Platform Considerations: Linux/WebKitGTK,
  Deps-Zeilen Arch/Debian/Fedora; macOS „WebKit is native"; Windows/WebView2)
- `tray-icon` 0.26.1 — <https://docs.rs/crate/tray-icon/0.26.1> (Linux/BSD-Deps, Default-Feature
  `libappindicator` vs. `ksni`)
- `rusqlite` 0.40.2 — <https://docs.rs/crate/rusqlite/0.40.2> (`bundled` nutzt `cc`, mitgeliefertes
  SQLite; Windows-Empfehlung)
- `notify-rust` 4.18.2 — <https://docs.rs/notify-rust/4.18.2/notify_rust/> (D-Bus/`zbus` auf Linux,
  `tauri-winrt-notification` auf Windows, `mac-notification-sys` auf macOS)
- `eframe` 0.31.1 — <https://docs.rs/eframe/0.31.1/eframe/> (Default-Features `glow`, `x11`, `wayland`)
  und `eframe_template` — <https://github.com/emilk/eframe_template> (Linux-`apt`/`dnf`-Paketlisten)
- `dirs` 6.0.0 `config_dir()` — <https://docs.rs/dirs/6.0.0/dirs/fn.config_dir.html>
- rustup, MSVC-Voraussetzungen — <https://rust-lang.github.io/rustup/installation/windows-msvc.html>
