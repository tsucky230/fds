//! `[ops] delete` (R-402-3, M1-a): how F8 deletes, and what an unclear value means.

use fds::ops::DeleteMode;
use fds::settings::{delete_mode, load_settings};

fn value(toml_text: &str) -> toml::Value {
    let table: toml::Table = toml::from_str(&format!("v = {toml_text}")).unwrap();
    table["v"].clone()
}

#[test]
fn missing_setting_keeps_permanent_deletion() {
    assert_eq!(delete_mode(None), (DeleteMode::Permanent, None));
}

#[test]
fn known_values_in_any_case_and_with_spaces() {
    for (text, mode) in [
        ("\"permanent\"", DeleteMode::Permanent),
        ("\"Permanent\"", DeleteMode::Permanent),
        ("\" PERMANENT \"", DeleteMode::Permanent),
        ("\"trash\"", DeleteMode::Trash),
        ("\"Trash\"", DeleteMode::Trash),
        ("\"  trash\"", DeleteMode::Trash),
    ] {
        assert_eq!(delete_mode(Some(&value(text))), (mode, None), "{text}");
    }
}

#[test]
fn unclear_values_move_to_the_trash_and_are_reported() {
    // Never fall back to permanent deletion on a value FDS does not understand.
    for text in [
        "\"trsh\"",
        "\"\"",
        "\"delete\"",
        "\"recycle\"",
        "1",
        "0",
        "true",
        "false",
        "[\"trash\"]",
        "{ mode = \"trash\" }",
    ] {
        let (mode, warning) = delete_mode(Some(&value(text)));
        assert_eq!(mode, DeleteMode::Trash, "{text}");
        let warning = warning.unwrap_or_else(|| panic!("{text}: not reported"));
        assert!(!warning.is_empty(), "{text}");
    }
    let (_, warning) = delete_mode(Some(&value("\"trsh\"")));
    assert!(warning.unwrap().contains("trsh"), "the value is shown");
}

#[test]
fn the_setting_is_read_from_the_config_file() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("config.toml"),
        "[ui]\nlanguage = \"en\"\n\n[ops]\ndelete = \"trash\"\n",
    )
    .unwrap();
    let settings = load_settings(Some(tmp.path()));
    assert_eq!(
        delete_mode(settings.ops.delete.as_ref()),
        (DeleteMode::Trash, None)
    );
    // A wrong type in [ops] must not throw away the rest of the config.
    std::fs::write(
        tmp.path().join("config.toml"),
        "[ui]\nlanguage = \"ja\"\n\n[ops]\ndelete = 3\n",
    )
    .unwrap();
    let settings = load_settings(Some(tmp.path()));
    assert_eq!(settings.ui.language.as_deref(), Some("ja"));
    assert_eq!(
        delete_mode(settings.ops.delete.as_ref()).0,
        DeleteMode::Trash
    );
}

#[test]
fn packs_and_providers_settings_are_read() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("config.toml"),
        "[packs]\ndisabled = [\"pdf-text\", \"old\"]\n\n[providers]\ntimeout_secs = 30\n",
    )
    .unwrap();
    let settings = load_settings(Some(tmp.path()));
    assert_eq!(settings.packs.disabled, ["pdf-text", "old"]);
    assert_eq!(settings.providers.timeout_secs, Some(30));
    let none = load_settings(None);
    assert!(none.packs.disabled.is_empty());
    assert_eq!(none.providers.timeout_secs, None);
}

#[test]
fn the_preview_setting_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    // (config text, [ui] preview)
    for (config, preview) in [
        ("[ui]\npreview = false\n", Some(false)),
        ("[ui]\npreview = true\n", Some(true)),
        ("[ui]\nframe = false\n", None),
        ("", None),
    ] {
        std::fs::write(tmp.path().join("config.toml"), config).unwrap();
        assert_eq!(
            load_settings(Some(tmp.path())).ui.preview,
            preview,
            "{config}"
        );
    }
}

#[test]
fn the_vi_keys_setting_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    // (config text, [viewer] vi_keys)
    for (config, vi_keys) in [
        ("[viewer]\nvi_keys = true\n", Some(true)),
        ("[viewer]\nvi_keys = false\n", Some(false)),
        ("[viewer]\nimage_protocol = \"none\"\n", None),
        ("", None),
    ] {
        std::fs::write(tmp.path().join("config.toml"), config).unwrap();
        assert_eq!(
            load_settings(Some(tmp.path())).viewer.vi_keys,
            vi_keys,
            "{config}"
        );
    }
}

#[test]
fn the_preview_images_setting_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    // (config text, [ui] preview_images)
    for (config, value) in [
        ("[ui]\npreview_images = false\n", Some(false)),
        ("[ui]\npreview_images = true\n", Some(true)),
        ("[ui]\npreview = false\n", None),
        ("", None),
    ] {
        std::fs::write(tmp.path().join("config.toml"), config).unwrap();
        assert_eq!(
            load_settings(Some(tmp.path())).ui.preview_images,
            value,
            "{config}"
        );
    }
}
