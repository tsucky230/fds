//! Opening files through the registry (P2-2, P2-4, P2-7, P2-9, P2-11, P2-14).

use std::fs;
use std::io::Cursor;
use std::path::Path;

use fds::viewer::detect::Sniff;
use fds::viewer::image::{ProtocolSetting, Resolved, font_size, resolve_protocol};
use fds::viewer::registry::Registry;
use fds::viewer::styled::StyledLine;
use fds::viewer::{
    Document, TEXT_LIMIT, TextDoc, ViewError, ViewerKind, ViewerProvider, open_document,
};

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn jpeg_bytes() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(8, 8, image::Rgb([10, 200, 10]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}

#[test]
fn candidates_per_file_type() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    let reg = Registry::default();
    let cases: [(&str, Vec<u8>, Vec<ViewerKind>); 4] = [
        (
            "a.txt",
            b"hello".to_vec(),
            vec![ViewerKind::Text, ViewerKind::Hex],
        ),
        (
            "a.md",
            b"# hi".to_vec(),
            vec![ViewerKind::Markdown, ViewerKind::Text, ViewerKind::Hex],
        ),
        ("a.bin", vec![0, 1, 2, 3], vec![ViewerKind::Hex]),
        (
            "pic.dat",
            png_bytes(4, 4),
            vec![ViewerKind::Image, ViewerKind::Hex],
        ),
    ];
    for (name, bytes, expected) in cases {
        fs::write(d.join(name), &bytes).unwrap();
        let opened = open_document(&reg, &d.join(name), None).unwrap();
        assert_eq!(opened.candidates, expected, "{name}");
        assert_eq!(opened.kind, expected[0], "{name}");
    }
}

#[test]
fn explicit_kind_and_text_content() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("r.md");
    fs::write(&p, "# T\r\n\r\nbody\r\n").unwrap();
    let reg = Registry::default();
    let opened = open_document(&reg, &p, Some(ViewerKind::Text)).unwrap();
    assert_eq!(opened.kind, ViewerKind::Text);
    let Document::Lines(doc) = opened.document else {
        panic!()
    };
    let texts: Vec<String> = doc.lines.iter().map(StyledLine::text).collect();
    assert_eq!(texts, ["# T", "", "body"]);
    assert_eq!(doc.encoding, "UTF-8");
    assert!(!doc.truncated);
}

#[test]
fn large_text_is_truncated_at_the_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("big.log");
    let line = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde\n"; // 64 bytes
    let count = (TEXT_LIMIT as usize / line.len()) + 16 * 1024; // ~9 MiB
    fs::write(&p, line.repeat(count)).unwrap();
    let opened = open_document(&Registry::default(), &p, None).unwrap();
    let Document::Lines(doc) = opened.document else {
        panic!()
    };
    assert!(doc.truncated);
    assert_eq!(doc.file_len, (line.len() * count) as u64);
    assert_eq!(doc.lines.len(), TEXT_LIMIT as usize / line.len());
}

#[test]
fn directories_and_missing_files_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = Registry::default();
    assert_eq!(
        open_document(&reg, tmp.path(), None).unwrap_err(),
        ViewError::NotAFile
    );
    assert!(matches!(
        open_document(&reg, &tmp.path().join("nope"), None),
        Err(ViewError::Io(_))
    ));
}

#[cfg(unix)]
#[test]
fn fifo_is_rejected_without_blocking() {
    let tmp = tempfile::tempdir().unwrap();
    let fifo = tmp.path().join("pipe");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success());
    // Would block forever if the FIFO were opened for reading.
    assert_eq!(
        open_document(&Registry::default(), &fifo, None).unwrap_err(),
        ViewError::NotAFile
    );
}

#[test]
fn images_decode_and_broken_images_fall_back_to_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = Registry::default();
    for (name, bytes) in [("a.png", png_bytes(40, 20)), ("b.jpg", jpeg_bytes())] {
        let p = tmp.path().join(name);
        fs::write(&p, bytes).unwrap();
        let Document::Image(doc) = open_document(&reg, &p, None).unwrap().document else {
            panic!("{name}")
        };
        assert!(doc.image.is_some(), "{name}");
        assert!(doc.problem.is_none(), "{name}");
    }
    let p = tmp.path().join("a.png");
    let Document::Image(doc) = open_document(&reg, &p, None).unwrap().document else {
        panic!()
    };
    assert_eq!(doc.dimensions, Some((40, 20)));

    let mut broken = png_bytes(4, 4);
    broken.truncate(30);
    let p = tmp.path().join("broken.png");
    fs::write(&p, broken).unwrap();
    let opened = open_document(&reg, &p, None).unwrap();
    assert_eq!(opened.kind, ViewerKind::Image);
    let Document::Image(doc) = opened.document else {
        panic!()
    };
    assert!(doc.image.is_none());
    assert!(doc.problem.is_some());
}

#[test]
fn big_images_are_downscaled() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("wide.png");
    fs::write(&p, png_bytes(3000, 10)).unwrap();
    let Document::Image(doc) = open_document(&Registry::default(), &p, None)
        .unwrap()
        .document
    else {
        panic!()
    };
    assert_eq!(doc.dimensions, Some((3000, 10)));
    let img = doc.image.unwrap();
    assert!(img.width() <= fds::viewer::image::MAX_SIDE);
}

#[test]
fn protocol_settings() {
    assert_eq!(ProtocolSetting::parse(None), ProtocolSetting::Auto);
    assert_eq!(
        ProtocolSetting::parse(Some("SIXEL")),
        ProtocolSetting::Sixel
    );
    assert_eq!(ProtocolSetting::parse(Some("bogus")), ProtocolSetting::Auto);
}

/// P2-11 / P2-15: the protocol is chosen from config and environment only; the terminal is never
/// queried (a query thread can block on stdin forever and steal key presses).
#[test]
fn protocol_resolution_uses_config_then_environment() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    };
    let none = env(&[]);
    // Config wins.
    assert_eq!(
        resolve_protocol(ProtocolSetting::Sixel, &none),
        Resolved::Protocol(ProtocolSetting::Sixel)
    );
    assert_eq!(
        resolve_protocol(ProtocolSetting::None, &none),
        Resolved::MetadataOnly
    );
    assert_eq!(
        resolve_protocol(
            ProtocolSetting::Halfblocks,
            &env(&[("KITTY_WINDOW_ID", "1")])
        ),
        Resolved::Protocol(ProtocolSetting::Halfblocks)
    );
    // Auto: environment hints.
    let auto = ProtocolSetting::Auto;
    assert_eq!(
        resolve_protocol(auto, &env(&[("KITTY_WINDOW_ID", "1")])),
        Resolved::Protocol(ProtocolSetting::Kitty)
    );
    assert_eq!(
        resolve_protocol(auto, &env(&[("TERM", "xterm-kitty")])),
        Resolved::Protocol(ProtocolSetting::Kitty)
    );
    assert_eq!(
        resolve_protocol(auto, &env(&[("TERM_PROGRAM", "ghostty")])),
        Resolved::Protocol(ProtocolSetting::Kitty)
    );
    assert_eq!(
        resolve_protocol(auto, &env(&[("TERM_PROGRAM", "iTerm.app")])),
        Resolved::Protocol(ProtocolSetting::Iterm2)
    );
    assert_eq!(
        resolve_protocol(auto, &env(&[("TERM_PROGRAM", "WezTerm")])),
        Resolved::Protocol(ProtocolSetting::Iterm2)
    );
    // tmux hides the outer terminal: half blocks even if a hint leaks through.
    assert_eq!(
        resolve_protocol(
            auto,
            &env(&[
                ("TMUX", "/tmp/tmux-1000/default,1,0"),
                ("KITTY_WINDOW_ID", "1")
            ])
        ),
        Resolved::Protocol(ProtocolSetting::Halfblocks)
    );
    // Windows Terminal draws Sixel (confirmed by the user in M17): Sixel.
    assert_eq!(
        resolve_protocol(auto, &env(&[("WT_SESSION", "x")])),
        Resolved::Protocol(ProtocolSetting::Sixel)
    );
    // ...but not when tmux runs inside it.
    assert_eq!(
        resolve_protocol(auto, &env(&[("WT_SESSION", "x"), ("TMUX", "t")])),
        Resolved::Protocol(ProtocolSetting::Halfblocks)
    );
    // Nothing known (plain xterm etc.): half blocks.
    assert_eq!(
        resolve_protocol(auto, &none),
        Resolved::Protocol(ProtocolSetting::Halfblocks)
    );
}

#[test]
fn font_size_from_window_pixels() {
    assert_eq!(font_size(80, 24, 800, 480), (10, 20));
    assert_eq!(font_size(80, 24, 0, 0), (8, 16), "unknown pixel size");
    assert_eq!(font_size(0, 0, 800, 480), (8, 16));
    assert_eq!(
        font_size(1000, 1000, 10, 10),
        (8, 16),
        "less than one pixel per cell is not usable"
    );
}

/// A viewer added from outside: no change to App / event loop is needed (P2-14).
struct Upper;

impl ViewerProvider for Upper {
    fn kind(&self) -> ViewerKind {
        ViewerKind::Custom("upper")
    }
    fn accepts(&self, sniff: &Sniff) -> Option<u8> {
        (sniff.extension.as_deref() == Some("up")).then_some(200)
    }
    fn load(&self, path: &Path, _sniff: &Sniff) -> Result<Document, ViewError> {
        let text = fs::read_to_string(path).map_err(|e| ViewError::Io(e.to_string()))?;
        Ok(Document::Lines(TextDoc {
            lines: vec![StyledLine::plain(text.to_uppercase())],
            encoding: "UTF-8".into(),
            truncated: false,
            file_len: text.len() as u64,
        }))
    }
}

#[test]
fn custom_provider_plugs_in() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("x.up");
    fs::write(&p, "shout").unwrap();
    let mut reg = Registry::default();
    reg.register(Box::new(Upper));
    let opened = open_document(&reg, &p, None).unwrap();
    assert_eq!(opened.kind, ViewerKind::Custom("upper"));
    assert_eq!(opened.candidates[0], ViewerKind::Custom("upper"));
    let Document::Lines(doc) = opened.document else {
        panic!()
    };
    assert_eq!(doc.lines[0].text(), "SHOUT");
}

// ---------- forced encoding (E, M1-b) ----------

fn sjis(text: &str) -> Vec<u8> {
    encoding_rs::SHIFT_JIS.encode(text).0.into_owned()
}

fn lines_of(opened: &fds::viewer::Opened) -> (String, String) {
    let Document::Lines(doc) = &opened.document else {
        panic!("expected lines, got {:?}", opened.kind)
    };
    let text: String = doc.lines.iter().map(|l| l.text()).collect();
    (text, doc.encoding.clone())
}

#[test]
fn a_forced_encoding_reaches_text_and_markdown() {
    use fds::viewer::open_document_with;
    let tmp = tempfile::tempdir().unwrap();
    let txt = tmp.path().join("a.txt");
    fs::write(&txt, sjis("日本語のテキストです。漢字とかな。")).unwrap();
    let md = tmp.path().join("m.md");
    fs::write(&md, encoding_rs::EUC_JP.encode("# 見出し\n\n本文です。").0).unwrap();
    let reg = Registry::default();

    let auto = open_document_with(&reg, &txt, None, None).unwrap();
    assert_eq!(auto.encoding, None);
    assert_eq!(lines_of(&auto).1, "Shift_JIS");

    let forced = open_document_with(
        &reg,
        &txt,
        Some(ViewerKind::Text),
        Some(encoding_rs::EUC_JP),
    )
    .unwrap();
    assert_eq!(forced.kind, ViewerKind::Text);
    assert_eq!(forced.encoding, Some(encoding_rs::EUC_JP));
    let (text, name) = lines_of(&forced);
    assert_eq!(name, "EUC-JP");
    assert!(
        !text.contains("日本語"),
        "decoded as EUC-JP, so not the original: {text}"
    );

    let back = open_document_with(
        &reg,
        &txt,
        Some(ViewerKind::Text),
        Some(encoding_rs::SHIFT_JIS),
    )
    .unwrap();
    assert!(lines_of(&back).0.contains("日本語"));

    let markdown = open_document_with(
        &reg,
        &md,
        Some(ViewerKind::Markdown),
        Some(encoding_rs::EUC_JP),
    )
    .unwrap();
    assert_eq!(markdown.kind, ViewerKind::Markdown);
    let (text, name) = lines_of(&markdown);
    assert_eq!(name, "EUC-JP");
    assert!(text.contains("本文です"), "{text}");
}

#[test]
fn hex_ignores_a_forced_encoding() {
    use fds::viewer::open_document_with;
    let tmp = tempfile::tempdir().unwrap();
    let txt = tmp.path().join("a.txt");
    fs::write(&txt, b"hello").unwrap();
    let reg = Registry::default();
    let hex = open_document_with(
        &reg,
        &txt,
        Some(ViewerKind::Hex),
        Some(encoding_rs::UTF_16LE),
    )
    .unwrap();
    assert_eq!(hex.kind, ViewerKind::Hex);
    assert!(matches!(hex.document, Document::Hex(_)));
    assert_eq!(
        hex.encoding, None,
        "only Lines documents carry a forced encoding"
    );
}

#[test]
fn without_a_forced_encoding_it_matches_open_document() {
    use fds::viewer::open_document_with;
    let tmp = tempfile::tempdir().unwrap();
    let reg = Registry::default();
    for (name, bytes) in [
        ("a.txt", sjis("かな漢字")),
        ("b.md", b"# t\n".to_vec()),
        ("c.bin", vec![0, 1, 2, 3]),
    ] {
        let path = tmp.path().join(name);
        fs::write(&path, bytes).unwrap();
        let a = open_document(&reg, &path, None).unwrap();
        let b = open_document_with(&reg, &path, None, None).unwrap();
        assert_eq!((a.kind, a.candidates), (b.kind, b.candidates), "{name}");
        assert_eq!(b.encoding, None);
    }
}

#[test]
fn a_truncated_file_can_be_forced_without_panic() {
    use fds::viewer::open_document_with;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("big.txt");
    // Larger than TEXT_LIMIT, with a multibyte char cut at the limit.
    let mut bytes = vec![b'a'; TEXT_LIMIT as usize - 1];
    bytes.extend("あいう".as_bytes());
    fs::write(&path, &bytes).unwrap();
    let reg = Registry::default();
    for enc in [
        encoding_rs::UTF_8,
        encoding_rs::UTF_16BE,
        encoding_rs::SHIFT_JIS,
    ] {
        let opened = open_document_with(&reg, &path, Some(ViewerKind::Text), Some(enc)).unwrap();
        let Document::Lines(doc) = &opened.document else {
            panic!()
        };
        assert!(doc.truncated);
        assert_eq!(doc.encoding, enc.name());
    }
}
