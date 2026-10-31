//! Preview pane loader (R-200-9): `preview::load_preview` on real files.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use fds::entry::EntryKind;
use fds::preview::{PREVIEW_DIR_LIMIT, PREVIEW_TEXT_LIMIT, PreviewDoc, PreviewError, load_preview};
use fds::viewer::ViewerKind;
use fds::viewer::detect::ImageFormat;
use fds::viewer::styled::StyledLine;

const JP: &str = "日本語のテキストです。漢字とかなを含みます。";

fn load(path: &Path, show_hidden: bool) -> Result<PreviewDoc, PreviewError> {
    load_preview(path, show_hidden, &AtomicBool::new(false))
}

fn texts(lines: &[StyledLine]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
        .collect()
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([10, 200, 10]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}

fn names(doc: &PreviewDoc) -> (Vec<String>, bool) {
    let PreviewDoc::Folder { entries, more } = doc else {
        panic!("not a folder preview: {doc:?}")
    };
    (
        entries.iter().map(|e| e.display_name.clone()).collect(),
        *more,
    )
}

#[test]
fn utf8_text_is_shown_line_by_line() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("a.txt");
    fs::write(&p, "hello\nworld\n\tTab\n").unwrap();
    let PreviewDoc::Lines {
        kind,
        lines,
        encoding,
        truncated,
    } = load(&p, false).unwrap()
    else {
        panic!()
    };
    assert_eq!(kind, ViewerKind::Text);
    assert_eq!(texts(&lines), ["hello", "world", "        Tab"]);
    assert_eq!(encoding, "UTF-8");
    assert!(!truncated);
}

#[test]
fn legacy_japanese_encodings_are_decoded() {
    let tmp = tempfile::tempdir().unwrap();
    let text = JP.repeat(4);
    for name in ["Shift_JIS", "EUC-JP", "UTF-8"] {
        let enc = encoding_rs::Encoding::for_label(name.as_bytes()).unwrap();
        let (bytes, _, had_errors) = enc.encode(&text);
        assert!(!had_errors);
        let p = tmp.path().join(format!("{name}.txt"));
        fs::write(&p, &bytes).unwrap();
        let PreviewDoc::Lines {
            lines, encoding, ..
        } = load(&p, false).unwrap()
        else {
            panic!("{name}")
        };
        assert_eq!(encoding, name);
        assert_eq!(texts(&lines), std::slice::from_ref(&text), "{name}");
    }
}

#[test]
fn markdown_is_rendered_like_the_viewer() {
    let tmp = tempfile::tempdir().unwrap();
    let source = "# Title\n\nSome *body* text.\n";
    for name in ["a.md", "b.markdown", "C.MD"] {
        let p = tmp.path().join(name);
        fs::write(&p, source).unwrap();
        let PreviewDoc::Lines { kind, lines, .. } = load(&p, false).unwrap() else {
            panic!("{name}")
        };
        assert_eq!(kind, ViewerKind::Markdown, "{name}");
        assert_eq!(lines, fds::viewer::markdown::render(source), "{name}");
    }
}

#[test]
fn text_is_read_only_up_to_the_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let limit = PREVIEW_TEXT_LIMIT as usize;
    // (size in bytes, truncated)
    for (size, truncated) in [
        (0, false),
        (1, false),
        (limit - 1, false),
        (limit, false),
        (limit + 1, true),
        (limit * 16, true),
    ] {
        // Lines of 7 letters and a newline; the last line may be cut.
        let content: String = "abcdefg\n".chars().cycle().take(size).collect();
        let p = tmp.path().join(format!("{size}.txt"));
        fs::write(&p, &content).unwrap();
        let PreviewDoc::Lines {
            lines,
            truncated: got,
            ..
        } = load(&p, false).unwrap()
        else {
            panic!("{size}")
        };
        assert_eq!(got, truncated, "{size}");
        let read: usize = texts(&lines).iter().map(|l| l.len() + 1).sum();
        assert!(
            read <= limit + 1,
            "{size}: {read} bytes shown, more than the limit"
        );
        if size == 0 {
            assert!(lines.is_empty());
        }
    }
}

#[test]
fn control_characters_never_reach_the_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("esc.txt");
    fs::write(&p, "a\x1b[31mred\x07bell\x1b]0;title\x07\n").unwrap();
    let PreviewDoc::Lines { lines, .. } = load(&p, false).unwrap() else {
        panic!()
    };
    for line in texts(&lines) {
        assert!(
            !line.chars().any(|c| c.is_control()),
            "control character in {line:?}"
        );
    }
}

#[test]
fn binary_files_keep_only_the_head_for_hex() {
    let tmp = tempfile::tempdir().unwrap();
    // (file size, bytes kept)
    for size in [1usize, 300_000] {
        let bytes: Vec<u8> = (0..size)
            .map(|i| if i % 3 == 0 { 0 } else { i as u8 })
            .collect();
        let p = tmp.path().join(format!("{size}.bin"));
        fs::write(&p, &bytes).unwrap();
        assert_eq!(
            load(&p, false).unwrap(),
            PreviewDoc::Binary {
                file_len: size as u64
            },
            "{size}"
        );
    }
}

#[test]
fn images_report_format_and_size_without_decoding() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, bytes, format, dims) in [
        ("a.png", png_bytes(40, 20), ImageFormat::Png, (40, 20)),
        ("b.png", png_bytes(1, 1), ImageFormat::Png, (1, 1)),
        ("c.jpg", jpeg_bytes(8, 16), ImageFormat::Jpeg, (8, 16)),
        // The content decides, not the extension.
        ("d.txt", png_bytes(3, 5), ImageFormat::Png, (3, 5)),
    ] {
        let p = tmp.path().join(name);
        fs::write(&p, &bytes).unwrap();
        assert_eq!(
            load(&p, false).unwrap(),
            PreviewDoc::Image {
                format,
                dimensions: Some(dims),
                file_len: bytes.len() as u64
            },
            "{name}"
        );
    }
}

#[test]
fn a_broken_image_header_gives_no_size_but_no_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut broken = png_bytes(4, 4);
    broken.truncate(12);
    let p = tmp.path().join("broken.png");
    fs::write(&p, &broken).unwrap();
    assert_eq!(
        load(&p, false).unwrap(),
        PreviewDoc::Image {
            format: ImageFormat::Png,
            dimensions: None,
            file_len: 12
        }
    );
}

#[test]
fn a_file_a_pack_provider_would_claim_is_still_previewed_built_in() {
    // External providers (packs) claim files by extension; the preview never asks them.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("report.pdf");
    fs::write(&p, "plain words\n").unwrap();
    let PreviewDoc::Lines { kind, lines, .. } = load(&p, false).unwrap() else {
        panic!()
    };
    assert_eq!(kind, ViewerKind::Text);
    assert_eq!(texts(&lines), ["plain words"]);
}

#[test]
fn folders_list_entries_folders_first_in_natural_order() {
    let tmp = tempfile::tempdir().unwrap();
    for f in ["b", "a10", "a2", ".hidden"] {
        fs::write(tmp.path().join(f), "x").unwrap();
    }
    fs::create_dir(tmp.path().join("zdir")).unwrap();
    fs::create_dir(tmp.path().join("Adir")).unwrap();

    let doc = load(tmp.path(), false).unwrap();
    assert_eq!(
        names(&doc),
        (
            vec![
                "Adir".to_string(),
                "zdir".into(),
                "a2".into(),
                "a10".into(),
                "b".into()
            ],
            false
        )
    );
    let PreviewDoc::Folder { entries, .. } = &doc else {
        panic!()
    };
    assert_eq!(entries[0].kind, EntryKind::Directory);
    assert_eq!(entries[2].kind, EntryKind::File);
    assert!(entries.iter().all(|e| !e.is_parent_link()));

    let (shown, _) = names(&load(tmp.path(), true).unwrap());
    assert_eq!(shown.len(), 6);
    assert!(shown.contains(&".hidden".to_string()));
}

#[test]
fn folders_are_cut_at_the_limit() {
    // (visible entries, hidden entries, kept, more)
    for (visible, hidden, kept, more) in [
        (0, 0, 0, false),
        (0, 3, 0, false),
        (1, 0, 1, false),
        (PREVIEW_DIR_LIMIT - 1, 0, PREVIEW_DIR_LIMIT - 1, false),
        (PREVIEW_DIR_LIMIT, 0, PREVIEW_DIR_LIMIT, false),
        (PREVIEW_DIR_LIMIT, 5, PREVIEW_DIR_LIMIT, false),
        (PREVIEW_DIR_LIMIT + 1, 0, PREVIEW_DIR_LIMIT, true),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..visible {
            fs::write(tmp.path().join(format!("f{i}")), "").unwrap();
        }
        for i in 0..hidden {
            fs::write(tmp.path().join(format!(".h{i}")), "").unwrap();
        }
        let (shown, got_more) = names(&load(tmp.path(), false).unwrap());
        assert_eq!(
            (shown.len(), got_more),
            (kept, more),
            "{visible} visible, {hidden} hidden"
        );
    }
}

#[test]
fn missing_paths_are_errors_not_panics() {
    let tmp = tempfile::tempdir().unwrap();
    for p in [
        tmp.path().join("missing.txt"),
        tmp.path().join("missing-dir"),
        tmp.path().join("missing-dir").join("a.txt"),
    ] {
        assert!(matches!(load(&p, false), Err(PreviewError::Io(_))), "{p:?}");
    }
}

#[test]
fn a_set_cancel_flag_stops_before_reading() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("a.txt");
    fs::write(&file, "text").unwrap();
    let cancel = AtomicBool::new(true);
    for p in [file.as_path(), tmp.path(), &tmp.path().join("missing")] {
        assert_eq!(
            load_preview(p, false, &cancel),
            Err(PreviewError::Cancelled),
            "{p:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn fifo_is_not_opened() {
    let tmp = tempfile::tempdir().unwrap();
    let fifo = tmp.path().join("pipe");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success());
    // Would block forever if the FIFO were opened for reading.
    assert_eq!(load(&fifo, false), Err(PreviewError::NotAFile));
}

#[cfg(unix)]
#[test]
fn unreadable_files_and_folders_are_errors_not_panics() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("secret.txt");
    fs::write(&file, "x").unwrap();
    let dir = tmp.path().join("locked");
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
    let file_result = load(&file, false);
    let dir_result = load(&dir, false);
    // Restore so the temp dir can be removed.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        matches!(file_result, Err(PreviewError::Io(_))),
        "{file_result:?}"
    );
    assert!(
        matches!(dir_result, Err(PreviewError::Io(_))),
        "{dir_result:?}"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_followed_and_broken_ones_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target.txt");
    fs::write(&target, "linked\n").unwrap();
    let link = tmp.path().join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let dir_link = tmp.path().join("dirlink");
    std::os::unix::fs::symlink(tmp.path(), &dir_link).unwrap();
    let broken = tmp.path().join("broken");
    std::os::unix::fs::symlink(tmp.path().join("gone"), &broken).unwrap();

    let PreviewDoc::Lines { lines, .. } = load(&link, false).unwrap() else {
        panic!()
    };
    assert_eq!(texts(&lines), ["linked"]);
    assert!(matches!(
        load(&dir_link, false),
        Ok(PreviewDoc::Folder { .. })
    ));
    assert!(matches!(load(&broken, false), Err(PreviewError::Io(_))));
}

// ---------- pictures for the pane (R-200-10) ----------

fn load_pictures(path: &Path) -> Result<PreviewDoc, PreviewError> {
    fds::preview::load_preview_with(path, false, true, &AtomicBool::new(false))
}

#[test]
fn thumbnails_off_is_exactly_load_preview() {
    let tmp = tempfile::tempdir().unwrap();
    let files = [
        ("a.png", png_bytes(40, 20)),
        ("b.jpg", jpeg_bytes(8, 16)),
        ("c.txt", b"text\n".to_vec()),
        ("d.bin", vec![0, 1, 2, 0, 3]),
    ];
    for (name, bytes) in &files {
        fs::write(tmp.path().join(name), bytes).unwrap();
    }
    let mut paths: Vec<_> = files.iter().map(|(n, _)| tmp.path().join(n)).collect();
    paths.push(tmp.path().to_path_buf());
    for p in paths {
        assert_eq!(
            fds::preview::load_preview_with(&p, false, false, &AtomicBool::new(false)),
            load(&p, false),
            "{p:?}"
        );
    }
}

#[test]
fn images_are_decoded_and_shrunk_for_the_pane() {
    use fds::preview::PREVIEW_THUMB_SIDE;
    let tmp = tempfile::tempdir().unwrap();
    // (name, bytes, format, original size)
    for (name, bytes, format, (w, h)) in [
        ("a.png", png_bytes(40, 20), ImageFormat::Png, (40, 20)),
        ("b.png", png_bytes(1, 1), ImageFormat::Png, (1, 1)),
        ("c.png", png_bytes(512, 100), ImageFormat::Png, (512, 100)),
        ("d.png", png_bytes(513, 100), ImageFormat::Png, (513, 100)),
        ("e.png", png_bytes(3000, 10), ImageFormat::Png, (3000, 10)),
        ("f.png", png_bytes(10, 3000), ImageFormat::Png, (10, 3000)),
        ("g.jpg", jpeg_bytes(8, 16), ImageFormat::Jpeg, (8, 16)),
        // The content decides, not the extension.
        ("h.txt", png_bytes(3, 5), ImageFormat::Png, (3, 5)),
    ] {
        let p = tmp.path().join(name);
        fs::write(&p, &bytes).unwrap();
        let Ok(PreviewDoc::Picture {
            format: got_format,
            dimensions,
            file_len,
            thumbnail,
        }) = load_pictures(&p)
        else {
            panic!("{name}: not a picture")
        };
        assert_eq!(got_format, format, "{name}");
        assert_eq!(dimensions, (w, h), "{name}");
        assert_eq!(file_len, bytes.len() as u64, "{name}");
        let (tw, th) = (thumbnail.0.width(), thumbnail.0.height());
        let longer = w.max(h);
        assert_eq!(
            tw.max(th),
            longer.min(PREVIEW_THUMB_SIDE),
            "{name}: {tw}x{th}"
        );
        assert!(tw >= 1 && th >= 1, "{name}: {tw}x{th}");
        if longer <= PREVIEW_THUMB_SIDE {
            assert_eq!((tw, th), (w, h), "{name}: kept as it is");
        }
        // The longer side stays the longer side (aspect ratio kept).
        assert_eq!(tw >= th, w >= h, "{name}: {tw}x{th}");
    }
}

#[test]
fn undecodable_or_too_large_images_show_metadata_like_load_preview() {
    use fds::preview::PREVIEW_IMAGE_LIMIT;
    let tmp = tempfile::tempdir().unwrap();
    let mut header_only = png_bytes(4, 4);
    header_only.truncate(12);
    let mut cut = png_bytes(4, 4);
    cut.truncate(40);
    let wide = png_bytes(fds::viewer::image::MAX_DECODE_SIDE + 1, 1);
    let mut over = png_bytes(4, 4);
    over.resize(PREVIEW_IMAGE_LIMIT as usize + 1, 0);
    for (name, bytes) in [
        ("header_only.png", header_only),
        ("cut.png", cut),
        ("too_wide.png", wide),
        ("over_limit.png", over),
    ] {
        let p = tmp.path().join(name);
        fs::write(&p, &bytes).unwrap();
        let got = load_pictures(&p).unwrap();
        assert!(matches!(got, PreviewDoc::Image { .. }), "{name}: {got:?}");
        assert_eq!(got, load(&p, false).unwrap(), "{name}");
    }
}

#[test]
fn a_picture_at_the_size_limit_is_still_decoded() {
    use fds::preview::PREVIEW_IMAGE_LIMIT;
    let tmp = tempfile::tempdir().unwrap();
    // Trailing bytes after the PNG's end are ignored by the decoder.
    let mut at_limit = png_bytes(4, 4);
    at_limit.resize(PREVIEW_IMAGE_LIMIT as usize, 0);
    let p = tmp.path().join("at_limit.png");
    fs::write(&p, &at_limit).unwrap();
    assert!(
        matches!(load_pictures(&p), Ok(PreviewDoc::Picture { .. })),
        "{:?}",
        load_pictures(&p)
    );
}

#[test]
fn a_set_cancel_flag_stops_picture_loads_too() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("a.png");
    fs::write(&p, png_bytes(40, 20)).unwrap();
    assert_eq!(
        fds::preview::load_preview_with(&p, false, true, &AtomicBool::new(true)),
        Err(PreviewError::Cancelled)
    );
}
