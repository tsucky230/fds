//! Language catalogs (R-1600).

use std::collections::{HashMap, HashSet};

use fds::i18n::{
    BUILTIN, Catalog, builtin, format_template, normalize_locale, parse_catalog, resolve_language,
};

fn available(codes: &[&str]) -> Vec<String> {
    codes.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_flattens_tables_and_keeps_strings_only() {
    let src = r#"
        top = "T"
        "quoted.key" = "Q"
        [status]
        counts = "{files} files"
        number = 3
        [a.b]
        c = "deep"
    "#;
    let map = parse_catalog(src).unwrap();
    assert_eq!(map.get("top").map(String::as_str), Some("T"));
    assert_eq!(map.get("quoted.key").map(String::as_str), Some("Q"));
    assert_eq!(
        map.get("status.counts").map(String::as_str),
        Some("{files} files")
    );
    assert_eq!(map.get("a.b.c").map(String::as_str), Some("deep"));
    assert!(!map.contains_key("status.number"));
}

#[test]
fn parse_reports_syntax_errors() {
    assert!(parse_catalog("this is = = not toml").is_err());
    assert!(parse_catalog("").unwrap().is_empty());
}

#[test]
fn template_substitutes_placeholders() {
    assert_eq!(
        format_template("{a} and {b}", &[("a", "1"), ("b", "2")]),
        "1 and 2"
    );
    assert_eq!(format_template("{a}{a}", &[("a", "x")]), "xx");
    assert_eq!(format_template("{missing} stays", &[]), "{missing} stays");
    assert_eq!(
        format_template("{{literal}} {a}", &[("a", "v")]),
        "{literal} v"
    );
    assert_eq!(format_template("open { brace", &[]), "open { brace");
    assert_eq!(format_template("close } brace", &[]), "close } brace");
    // Values are not expanded again.
    assert_eq!(format_template("{a}", &[("a", "{b}"), ("b", "no")]), "{b}");
    // Multibyte text around placeholders.
    assert_eq!(format_template("{n} 個マーク", &[("n", "3")]), "3 個マーク");
}

#[test]
fn locale_normalization() {
    assert_eq!(normalize_locale("ja_JP.UTF-8").as_deref(), Some("ja-jp"));
    assert_eq!(normalize_locale("en-US").as_deref(), Some("en-us"));
    assert_eq!(normalize_locale("de_DE@euro").as_deref(), Some("de-de"));
    assert_eq!(normalize_locale("ja").as_deref(), Some("ja"));
    assert_eq!(normalize_locale("C"), None);
    assert_eq!(normalize_locale("POSIX"), None);
    assert_eq!(normalize_locale(""), None);
}

#[test]
fn language_resolution_order() {
    let avail = available(&["en", "ja", "pt-br"]);
    // Config wins over OS.
    assert_eq!(
        resolve_language(Some("ja"), Some("en_US.UTF-8"), &avail),
        "ja"
    );
    // OS locale when no config; region falls back to the base language.
    assert_eq!(resolve_language(None, Some("ja_JP.UTF-8"), &avail), "ja");
    // Full code when available.
    assert_eq!(resolve_language(None, Some("pt_BR"), &avail), "pt-br");
    // Unknown config falls through to the OS locale.
    assert_eq!(resolve_language(Some("xx"), Some("ja-JP"), &avail), "ja");
    // Nothing usable -> English.
    assert_eq!(resolve_language(None, Some("C"), &avail), "en");
    assert_eq!(resolve_language(None, None, &avail), "en");
    assert_eq!(resolve_language(Some("fr"), Some("de_DE"), &avail), "en");
}

#[test]
fn catalog_falls_back_to_english_then_key() {
    let texts = HashMap::from([("a".to_string(), "日本語 {x}".to_string())]);
    let fallback = HashMap::from([
        ("a".to_string(), "en {x}".to_string()),
        ("b".to_string(), "only en".to_string()),
    ]);
    let c = Catalog::new("ja", texts, fallback);
    assert_eq!(c.text("a", &[("x", "1")]), "日本語 1");
    assert_eq!(c.text("b", &[]), "only en");
    assert_eq!(c.text("no.such.key", &[]), "no.such.key");
    assert_eq!(c.language(), "ja");
}

#[test]
fn builtin_files_parse_and_cover_english_keys() {
    let en: HashSet<String> = parse_catalog(builtin("en").unwrap())
        .unwrap()
        .into_keys()
        .collect();
    assert!(en.contains("status.counts"));
    for (code, source) in BUILTIN {
        let keys: HashSet<String> = parse_catalog(source)
            .unwrap_or_else(|e| panic!("{code}: {e}"))
            .into_keys()
            .collect();
        let missing: Vec<_> = en.difference(&keys).collect();
        assert!(missing.is_empty(), "{code} is missing keys: {missing:?}");
    }
}

#[test]
fn short_key_guide_shows_quit_at_the_narrowest_width() {
    // The short guide is used from 40 columns up; how to quit must be within those columns.
    use unicode_width::UnicodeWidthChar;
    for (code, source) in BUILTIN {
        let texts = parse_catalog(source).unwrap();
        let short = &texts["guide.short"];
        let mut width = 0;
        let head: String = short
            .chars()
            .take_while(|c| {
                width += c.width().unwrap_or(0);
                width <= 40
            })
            .collect();
        assert!(head.contains(" q "), "{code}: {short:?} -> {head:?}");
    }
}

#[test]
fn help_labels_do_not_mention_phases() {
    // Every listed feature exists now; "(Phase 3)" would read as "not available yet".
    for (code, source) in BUILTIN {
        let texts = parse_catalog(source).unwrap();
        for (key, value) in texts.iter().filter(|(k, _)| k.starts_with("help.")) {
            assert!(!value.contains("Phase"), "{code} {key} = {value}");
        }
    }
}

#[test]
fn english_catalog_renders_known_text() {
    let c = Catalog::english();
    assert_eq!(
        c.text("status.counts", &[("files", "5"), ("dirs", "2")]),
        "5 files   2 dirs"
    );
}
