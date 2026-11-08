use super::*;
use crate::about::LicenseTab;
use rune::{Cx, Env, Runtime, tok};

#[derive(Default)]
struct RouteFixture {
    page: LivePage,
    tab: LicenseTab,
}

impl AboutRouteHost for RouteFixture {
    fn page(&self) -> LivePage {
        self.page
    }

    fn set_page(&mut self, page: LivePage) {
        self.page = page;
    }

    fn license_tab(&self) -> LicenseTab {
        self.tab
    }

    fn set_license_tab(&mut self, tab: LicenseTab) {
        self.tab = tab;
    }
}

fn fixture_view(state: &RouteFixture, cx: &mut Cx<RouteFixture>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    let colors = crate::rune_theme::palette(dark);
    cx.foreground(theme.text);
    cx.focus_ring(2.0, 2.0, colors.focus_ring);
    let content = match state.page {
        LivePage::Backup => col()
            .key("about-fixture-backup")
            .w_full()
            .h_full()
            .center()
            .child(about_entry_button::<RouteFixture>(theme, colors)),
        LivePage::About | LivePage::Licenses => route_content(state, theme, colors),
        LivePage::Settings => el(),
    };
    col()
        .key("about-fixture-window")
        .role("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(content)
}

fn runtime(dark: bool) -> Runtime<RouteFixture> {
    let mut runtime = Runtime::new(
        RouteFixture::default(),
        fixture_view,
        (1180.0, 820.0),
        Env::new(dark, 1.0),
    );
    runtime.frame(0.0);
    runtime
}

fn click(runtime: &mut Runtime<RouteFixture>, key: &str) {
    let (x, y) = runtime
        .find(key)
        .unwrap_or_else(|| panic!("missing Rune About/license control {key:?}"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

#[test]
fn rune_about_and_license_routes_interact_without_production_state() {
    for dark in [false, true] {
        let mut runtime = runtime(dark);
        assert_eq!(runtime.state.page, LivePage::Backup);
        click(&mut runtime, "open-about");

        assert_eq!(runtime.state.page, LivePage::About);
        for key in [
            "rune-about-page",
            "about-product-name",
            "about-version",
            "license-open",
            "about-close",
            "about-copyright",
        ] {
            assert!(runtime.find(key).is_some(), "missing About node {key:?}");
        }

        click(&mut runtime, "license-open");
        assert_eq!(runtime.state.page, LivePage::Licenses);
        assert_eq!(runtime.state.tab, LicenseTab::Components);
        assert!(runtime.find("license-tab-components").is_some());
        assert!(runtime.find("license-tab-icons").is_some());
        assert!(runtime.find("license-line-0").is_some());

        let total_lines = crate::about::license_lines().len();
        assert!(total_lines > 10_000, "the full embedded license list is present");
        let built_rows = runtime
            .elements()
            .into_iter()
            .filter(|element| element.path.contains("/license-lines/license-line-"))
            .count();
        assert!(built_rows > 0 && built_rows < 250, "built {built_rows} of {total_lines} license rows");
        assert!(
            runtime.find("license-line-500").is_none(),
            "offscreen rows are not built eagerly"
        );

        click(&mut runtime, "license-tab-icons");
        assert_eq!(runtime.state.tab, LicenseTab::Icons);
        assert!(runtime.find("attribution-line-0").is_some());
        let last_attribution = crate::about::icon_attributions().lines().count() - 1;
        assert!(runtime.find(&format!("attribution-line-{last_attribution}")).is_some());

        click(&mut runtime, "license-tab-components");
        assert_eq!(runtime.state.tab, LicenseTab::Components);
        assert!(runtime.find("license-line-0").is_some());
        click(&mut runtime, "license-back-about");
        assert_eq!(runtime.state.page, LivePage::About);
        click(&mut runtime, "about-close");
        assert_eq!(runtime.state.page, LivePage::Backup);

        click(&mut runtime, "open-about");
        click(&mut runtime, "license-open");
        click(&mut runtime, "license-close");
        assert_eq!(runtime.state.page, LivePage::Backup);
    }
}
