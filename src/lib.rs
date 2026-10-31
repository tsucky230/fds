//! FDS - keyboard-first TUI file manager.
//!
//! Dependency direction: run -> (terminal, ui, fs) -> app -> (command, pane) -> (entry, sort, text, layout).
//! Everything from app down is pure logic that touches neither the terminal nor the filesystem.

pub mod app;
pub mod app_archive;
pub mod app_info;
pub mod app_ops;
pub mod app_preview;
pub mod archive;
pub mod cli;
pub mod command;
pub mod editor_cmd;
pub mod entry;
pub mod fs;
pub mod i18n;
pub mod info;
pub mod layout;
pub mod license;
pub mod line_editor;
pub mod message;
pub mod open;
pub mod ops;
pub mod pack;
pub mod pane;
pub mod preview;
pub mod run;
pub mod search;
pub mod settings;
pub mod shell_cmd;
pub mod sort;
pub mod supporter;
pub mod terminal;
pub mod text;
pub mod ui;
pub mod ui_archive;
pub mod ui_dest;
pub mod ui_info;
pub mod ui_input;
pub mod ui_preview;
pub mod ui_viewer;
pub mod viewer;
