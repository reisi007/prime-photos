//! Local-only interactive Wry child verification route for Issue #49.

use rune::tok;
use rune::{App, AppSpec, Cx, El, Tx, WindowHandle, col, el, row, text};
use wry::WebViewBuilder;

use crate::login;
use crate::wry_host;

const WINDOW_SIZE: (f32, f32) = (960.0, 680.0);

const LOCAL_SMOKE_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Prime Photos local Wry smoke</title>
  <style>
    :root { color-scheme: light dark; font: 16px -apple-system, BlinkMacSystemFont, sans-serif; }
    body { margin: 0; padding: 32px; background: #f1f4f4; color: #172123; }
    main { max-width: 720px; margin: 0 auto; }
    .badge { display: inline-block; padding: 7px 10px; border-radius: 99px; background: #d8f4e7; color: #17553a; font-size: 12px; font-weight: 700; letter-spacing: .04em; }
    h1 { margin: 22px 0 8px; font-size: 28px; }
    p { line-height: 1.5; }
    label { display: block; margin: 22px 0 7px; font-weight: 650; }
    textarea { box-sizing: border-box; display: block; width: 100%; min-height: 72px; padding: 12px; border: 1px solid #8b999b; border-radius: 9px; background: #fff; color: #172123; font: 15px ui-monospace, monospace; resize: vertical; }
    textarea:focus { outline: 3px solid #35a77a; outline-offset: 2px; }
    #status { min-height: 24px; padding: 10px 12px; border-radius: 8px; background: #e4e9e9; }
    @media (prefers-color-scheme: dark) {
      body { background: #202727; color: #ecf2f1; }
      textarea { border-color: #657374; background: #2d3636; color: #ecf2f1; }
      #status { background: #303a3a; }
      .badge { background: #244a39; color: #b9f1d3; }
    }
  </style>
</head>
<body>
  <main>
    <span class="badge">LOCAL ONLY · NO NETWORK · NO SIGN-IN</span>
    <h1>Prime Photos Wry smoke</h1>
    <p>This embedded page contains only synthetic text. Resize the native window, focus either field, and type to verify rendering, live bounds, keyboard input, and focus.</p>
    <label for="copy-source">Synthetic copy source</label>
    <textarea id="copy-source" spellcheck="false">Prime Photos synthetic clipboard sample 4827</textarea>
    <label for="paste-target">Keyboard and paste target</label>
    <textarea id="paste-target" placeholder="Click here and type synthetic text"></textarea>
    <p id="status" role="status">Select the sample, then use Edit → Copy; focus the second field and use Edit → Paste.</p>
  </main>
  <script>
    const status = document.getElementById('status');
    for (const field of document.querySelectorAll('textarea')) {
      field.addEventListener('focus', () => { status.textContent = `Focused: ${field.id}`; });
      field.addEventListener('input', () => { status.textContent = `Keyboard input received in ${field.id}`; });
    }
    document.getElementById('paste-target').addEventListener('paste', () => {
      status.textContent = 'Native Edit → Paste received by the local page.';
    });
  </script>
</body>
</html>"#;

#[derive(Default)]
struct SmokeState {
    native_window: Option<WindowHandle>,
    child_bounds: Option<[f32; 4]>,
    attempted: bool,
    started: bool,
    error: Option<String>,
}

pub(crate) fn run(args: Vec<String>) -> Result<(), String> {
    if rune::tooling::Args::parse_from(args.clone()).headless() {
        return Err("--rune-wry-smoke is an interactive local-only route".to_owned());
    }
    let spec = AppSpec {
        min_size: Some((640.0, 480.0)),
        poll: Some(poll),
        ..AppSpec::new("Prime Photos Wry smoke", WINDOW_SIZE, SmokeState::default, view)
    };
    let app = App::new(spec)
        .init(SmokeState::default)
        .args(args)
        .standard_menus()
        .on_close(close);
    let app_handle = app.handle();
    app.on_window(move |window| {
        let native_window = window.clone();
        app_handle.update(move |state, _| state.native_window = Some(native_window));
    })
    .try_run()
}

fn poll(state: &mut SmokeState, _: f64, _: &mut Tx) -> bool {
    if state.attempted {
        return false;
    }
    let (Some(native_window), Some(rect)) = (state.native_window.clone(), state.child_bounds) else {
        return false;
    };
    state.attempted = true;
    let bounds = login::rect(rect[0], rect[1], rect[2], rect[3]);
    match native_window.with_raw_handle(|raw| {
        let webview = WebViewBuilder::new()
            .with_bounds(bounds)
            .with_html(LOCAL_SMOKE_HTML)
            .build_as_child(&raw)
            .map_err(|error| error.to_string())?;
        wry_host::install(webview)
    }) {
        Ok(Ok(())) => state.started = true,
        Ok(Err(error)) | Err(error) => state.error = Some(error),
    }
    true
}

fn close(_: &mut SmokeState, _: &mut Tx) {
    wry_host::close();
}

fn view(state: &SmokeState, cx: &mut Cx<SmokeState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    let header = row()
        .key("wry-smoke-header")
        .w_full()
        .h(72.0)
        .pl(cx.titlebar_inset() + 26.0)
        .pr(24.0)
        .center_y()
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
                    text("Local Wry child-view smoke · no account or network")
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    let mut area = el()
        .key("wry-smoke-child-area")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .on_layout(|state: &mut SmokeState, rect| {
            let bounds = wry_host::bounds_from_layout(rect);
            state.child_bounds = Some(bounds);
            if state.started {
                wry_host::set_bounds(login::rect(
                    bounds[0], bounds[1], bounds[2], bounds[3],
                ));
            }
        });
    if let Some(error) = &state.error {
        area = area.child(
            text(format!("Local Wry view could not be created: {error}"))
                .key("wry-smoke-error")
                .color(tok::accent(2, dark)),
        );
    }
    col()
        .key("wry-smoke-window")
        .role("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(header)
        .child(area)
}

#[cfg(test)]
mod tests {
    use super::LOCAL_SMOKE_HTML;

    #[test]
    fn local_smoke_page_is_synthetic_and_has_no_network_urls_or_external_assets() {
        assert!(!LOCAL_SMOKE_HTML.contains("http://"));
        assert!(!LOCAL_SMOKE_HTML.contains("https://"));
        assert!(!LOCAL_SMOKE_HTML.contains("src="));
        assert!(!LOCAL_SMOKE_HTML.contains("href="));
        assert!(LOCAL_SMOKE_HTML.contains("LOCAL ONLY · NO NETWORK · NO SIGN-IN"));
        assert!(LOCAL_SMOKE_HTML.contains("Prime Photos synthetic clipboard sample 4827"));
        assert!(LOCAL_SMOKE_HTML.contains("id=\"copy-source\""));
        assert!(LOCAL_SMOKE_HTML.contains("id=\"paste-target\""));
        assert!(LOCAL_SMOKE_HTML.contains("addEventListener('focus'"));
        assert!(LOCAL_SMOKE_HTML.contains("addEventListener('paste'"));
    }
}
