//! q / F10 / Esc in the list ask before quitting (R-200-14).

use std::path::PathBuf;

use fds::app::{App, Effect, Mode};
use fds::command::{Command, TextKey};
use fds::i18n::Catalog;
use fds::ui::render;
use fds::ui_viewer::ImageSlot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn asking() -> App {
    let (mut app, effect) = App::new(PathBuf::from(if cfg!(windows) { r"C:\w" } else { "/w" }));
    let Effect::Load { id, .. } = effect else {
        panic!()
    };
    app.on_listing(id, Ok(vec![]));
    assert_eq!(app.handle(Command::Back), Effect::None);
    assert_eq!(app.mode, Mode::ConfirmQuit);
    assert!(app.wants_text());
    app
}

#[test]
fn y_and_enter_quit() {
    for key in [TextKey::Char('y'), TextKey::Char('Y'), TextKey::Enter] {
        assert_eq!(asking().handle_text(key), Effect::Quit);
    }
    let mut app = asking();
    app.handle_text(TextKey::Esc);
    assert_eq!(app.handle(Command::Quit), Effect::None);
    assert_eq!(app.mode, Mode::ConfirmQuit);
    assert_eq!(app.handle_text(TextKey::Enter), Effect::Quit);
}

#[test]
fn n_and_esc_go_back_and_other_keys_are_ignored() {
    for key in [TextKey::Char('n'), TextKey::Char('N'), TextKey::Esc] {
        let mut app = asking();
        assert_eq!(app.handle_text(key), Effect::None);
        assert_eq!(app.mode, Mode::Normal);
    }
    let mut app = asking();
    assert_eq!(app.handle_text(TextKey::Char('q')), Effect::None);
    assert_eq!(app.mode, Mode::ConfirmQuit);
}

fn last_row(app: &mut App, width: u16, height: u16, frame: bool) -> String {
    app.show_frame = frame;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let catalog = Catalog::english();
    terminal
        .draw(|f| render(f, app, &catalog, &mut ImageSlot::default()))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..width).map(|x| buf[(x, height - 1)].symbol()).collect()
}

#[test]
fn the_question_is_on_the_bottom_row() {
    // With the key guide, and on a low screen without one.
    for height in [24, 12] {
        let row = last_row(&mut asking(), 80, height, false);
        assert!(row.contains("Quit from FDS? (y/n)"), "{height}: {row}");
    }
}
