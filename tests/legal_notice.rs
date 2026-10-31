//! The license and the notices that go with it (ADR 0008).

use std::path::Path;

use fds::app::{App, Effect, Mode};
use fds::cli::help_text;
use fds::command::{Command, KEY_HELP};
use fds::i18n::{Catalog, builtin, parse_catalog};
use fds::ui::render;
use fds::ui_viewer::ImageSlot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const COPYRIGHT: &str = "© 2026 Qraft Node Kyoto";
const LICENSE_NAME: &str = "GPL-3.0-only";
const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";

fn japanese() -> Catalog {
    let ja = parse_catalog(builtin("ja").unwrap()).unwrap();
    let en = parse_catalog(builtin("en").unwrap()).unwrap();
    Catalog::new("ja", ja, en)
}

fn help_screen(catalog: &Catalog, width: u16, height: u16) -> String {
    let root = std::path::PathBuf::from(if cfg!(windows) { r"C:\w" } else { "/w" });
    let (mut app, effect) = App::new(root);
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(id, Ok(vec![]));
    app.handle(Command::Help);
    assert_eq!(app.mode, Mode::Help);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| render(f, &mut app, catalog, &mut ImageSlot::default()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_package_declares_the_license() {
    assert_eq!(env!("CARGO_PKG_LICENSE"), LICENSE_NAME);
}

#[test]
fn the_license_file_is_the_whole_gpl_3() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("LICENSE"))
        .expect("LICENSE at the top of the repository");
    assert!(text.contains("GNU GENERAL PUBLIC LICENSE"));
    assert!(text.contains("Version 3, 29 June 2007"));
    assert!(text.contains("END OF TERMS AND CONDITIONS"));
    // Unchanged from the text the FSF publishes, which is 35149 bytes with LF line ends.
    assert_eq!(text.replace("\r\n", "\n").len(), 35149);
}

#[test]
fn help_text_carries_the_notice_in_each_language() {
    for (catalog, no_warranty) in [
        (Catalog::english(), "NO WARRANTY"),
        (japanese(), "保証はありません"),
    ] {
        let text = help_text(&catalog, None);
        for needle in [COPYRIGHT, LICENSE_NAME, LICENSE_URL, no_warranty] {
            assert!(text.contains(needle), "{needle}\n{text}");
        }
    }
}

#[test]
fn the_help_screen_carries_the_notice_when_there_is_room() {
    let shown = help_screen(&Catalog::english(), 80, 60);
    for needle in [COPYRIGHT, LICENSE_NAME, LICENSE_URL, "NO WARRANTY"] {
        assert!(shown.contains(needle), "{needle}\n{shown}");
    }
    // A wide character is followed by a blank cell in the buffer.
    let shown = help_screen(&japanese(), 80, 60).replace(' ', "");
    for needle in [LICENSE_URL, "保証はありません"] {
        assert!(shown.contains(needle), "{needle}\n{shown}");
    }
}

#[test]
fn a_small_help_screen_still_lists_the_keys() {
    let shown = help_screen(&Catalog::english(), 80, 20);
    assert!(shown.contains(KEY_HELP[0].0), "{shown}");
    assert!(!shown.contains("NO WARRANTY"), "{shown}");
}
