//! External command providers (R-802, Phase 4: C27 to C31).
//!
//! Uses the fixture program `examples/provider_echo.rs`, which `cargo test` builds next to the
//! test binaries (target/<profile>/examples). It prints its arguments one per line.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use fds::pack::ProviderSpec;
use fds::viewer::detect::Sniff;
use fds::viewer::external::{
    EXTERNAL_PRIORITY, ExternalProvider, PATH_PLACEHOLDER, ProviderError, RunLimits, build_argv,
    run_provider, timeout_from_setting,
};
use fds::viewer::registry::Registry;
use fds::viewer::{Document, ViewError, ViewerKind, ViewerProvider, open_document_cancellable};

fn echo() -> String {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap().join("examples");
    let path = dir.join(format!("provider_echo{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.exists(),
        "{} is missing: run the whole `cargo test` (it builds examples) or `cargo build --example provider_echo`",
        path.display()
    );
    path.to_string_lossy().into_owned()
}

fn cmd(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

fn limits(timeout_ms: u64) -> RunLimits {
    RunLimits {
        timeout: Duration::from_millis(timeout_ms),
        max_stdout: 8 * 1024 * 1024,
        max_stderr: 4 * 1024,
    }
}

fn never() -> AtomicBool {
    AtomicBool::new(false)
}

fn file(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, "content").unwrap();
    p
}

fn lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::to_string)
        .collect()
}

// ---------- argv (C27) ----------

#[test]
fn only_whole_placeholders_become_the_absolute_path() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let abs = std::path::absolute(&path).unwrap();
    let argv = build_argv(
        &cmd(&[
            "tool",
            "-x",
            PATH_PLACEHOLDER,
            "--in={path}",
            PATH_PLACEHOLDER,
        ]),
        &path,
    );
    assert_eq!(argv.len(), 5);
    assert_eq!(argv[0], "tool");
    assert_eq!(argv[1], "-x");
    assert_eq!(argv[2], abs.as_os_str());
    assert_eq!(
        argv[3], "--in={path}",
        "a placeholder inside an argument stays literal"
    );
    assert_eq!(argv[4], abs.as_os_str());
}

#[test]
fn relative_and_option_like_names_are_passed_as_absolute_paths() {
    let tmp = tempfile::tempdir().unwrap();
    for name in ["-rf", "--help", "a b.pdf", "x;y&z$(id).pdf"] {
        let path = file(tmp.path(), name);
        let argv = build_argv(&cmd(&["tool", PATH_PLACEHOLDER]), &path);
        let arg = PathBuf::from(&argv[1]);
        assert!(arg.is_absolute(), "{name}: {arg:?}");
        assert!(!argv[1].to_string_lossy().starts_with('-'), "{name}");
        assert!(argv[1].to_string_lossy().ends_with(name), "{name}");
    }
}

#[test]
fn arguments_reach_the_program_exactly_without_a_shell() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "my file;&$x.pdf");
    let tricky = [
        "a b",
        "x;y&z",
        "$(whoami)",
        "`id`",
        "%PATH%",
        "\"quoted\"",
        "back\\slash",
        "",
        "*",
    ];
    let mut command = vec![echo()];
    command.extend(tricky.iter().map(|s| s.to_string()));
    command.push(PATH_PLACEHOLDER.to_string());
    let out = run_provider(&command, &path, &limits(10_000), &never()).unwrap();
    assert!(!out.truncated);
    let mut expected: Vec<String> = tricky.iter().map(|s| s.to_string()).collect();
    expected.push(
        std::path::absolute(&path)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    );
    assert_eq!(lines(&out.stdout), expected);
}

// ---------- failures (C28) ----------

#[test]
fn a_missing_program_is_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let result = run_provider(
        &cmd(&["fds-no-such-program-xyz", PATH_PLACEHOLDER]),
        &path,
        &limits(5_000),
        &never(),
    );
    assert_eq!(
        result,
        Err(ProviderError::NotFound("fds-no-such-program-xyz".into()))
    );
}

#[test]
fn a_slow_program_is_killed_at_the_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let start = Instant::now();
    let result = run_provider(
        &[
            echo(),
            "--sleep-ms".into(),
            "10000".into(),
            PATH_PLACEHOLDER.into(),
        ],
        &path,
        &limits(400),
        &never(),
    );
    assert_eq!(
        result,
        Err(ProviderError::Timeout(Duration::from_millis(400)))
    );
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn cancel_kills_the_program() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Relaxed);
    });
    let start = Instant::now();
    let result = run_provider(
        &[
            echo(),
            "--sleep-ms".into(),
            "10000".into(),
            PATH_PLACEHOLDER.into(),
        ],
        &path,
        &limits(30_000),
        &cancel,
    );
    assert_eq!(result, Err(ProviderError::Cancelled));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn a_failing_program_reports_its_exit_code_and_stderr() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let result = run_provider(
        &[
            echo(),
            "--stderr".into(),
            "boom: cannot read".into(),
            "--exit".into(),
            "3".into(),
            PATH_PLACEHOLDER.into(),
        ],
        &path,
        &limits(10_000),
        &never(),
    );
    let Err(ProviderError::Failed { code, stderr }) = result else {
        panic!("{result:?}")
    };
    assert_eq!(code, Some(3));
    assert!(stderr.contains("boom: cannot read"), "{stderr:?}");
}

#[test]
fn huge_output_is_cut_and_the_program_stopped() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let small = RunLimits {
        max_stdout: 1000,
        ..limits(10_000)
    };
    let start = Instant::now();
    let out = run_provider(
        &[
            echo(),
            "--bytes".into(),
            "50000000".into(),
            PATH_PLACEHOLDER.into(),
        ],
        &path,
        &small,
        &never(),
    )
    .unwrap();
    assert!(out.truncated);
    assert_eq!(out.stdout.len(), 1000);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn error_messages_say_what_happened() {
    let cases = [
        (ProviderError::NotFound("pdftotext".into()), "pdftotext"),
        (ProviderError::Timeout(Duration::from_secs(10)), "10"),
        (
            ProviderError::Failed {
                code: Some(3),
                stderr: "bad file".into(),
            },
            "bad file",
        ),
        (ProviderError::Cancelled, "cancel"),
        (ProviderError::Io("denied".into()), "denied"),
    ];
    for (error, part) in cases {
        let text = error.message();
        assert!(text.contains("provider"), "{text}");
        assert!(text.contains(part), "{text}");
    }
    assert!(
        ProviderError::Failed {
            code: Some(3),
            stderr: String::new()
        }
        .message()
        .contains('3')
    );
}

// ---------- settings ----------

#[test]
fn timeout_setting_is_clamped() {
    for (setting, secs) in [
        (None, 10),
        (Some(0), 1),
        (Some(1), 1),
        (Some(2), 2),
        (Some(30), 30),
        (Some(299), 299),
        (Some(300), 300),
        (Some(301), 300),
        (Some(u64::MAX), 300),
    ] {
        assert_eq!(
            timeout_from_setting(setting),
            Duration::from_secs(secs),
            "{setting:?}"
        );
    }
}

// ---------- as a viewer (C29 to C31) ----------

fn spec(label: &str, extensions: &[&str], command: Vec<String>) -> ProviderSpec {
    ProviderSpec {
        id: label.to_lowercase().replace(' ', "-"),
        label: label.into(),
        extensions: extensions.iter().map(|e| e.to_string()).collect(),
        command,
    }
}

fn sniff_for(path: &Path) -> Sniff {
    let len = std::fs::metadata(path).unwrap().len();
    fds::viewer::read_sniff(path, len).unwrap()
}

#[test]
fn it_takes_its_extensions_first_and_tab_reaches_the_built_in_viewers() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = ExternalProvider::new(
        spec(
            "Echo text",
            &["txt", "pdf"],
            vec![echo(), PATH_PLACEHOLDER.into()],
        ),
        Duration::from_secs(10),
    );
    assert_eq!(provider.kind(), ViewerKind::Custom("Echo text"));
    let txt = file(tmp.path(), "a.TXT");
    let md = file(tmp.path(), "b.md");
    assert_eq!(provider.accepts(&sniff_for(&txt)), Some(EXTERNAL_PRIORITY));
    assert_eq!(provider.accepts(&sniff_for(&md)), None);
    let mut registry = Registry::default();
    registry.register(Box::new(provider));
    let candidates = registry.candidates(&sniff_for(&txt));
    assert_eq!(candidates[0], ViewerKind::Custom("Echo text"));
    assert!(candidates.contains(&ViewerKind::Text), "{candidates:?}");
    assert!(candidates.contains(&ViewerKind::Hex), "{candidates:?}");
    assert!(
        !registry
            .candidates(&sniff_for(&md))
            .contains(&ViewerKind::Custom("Echo text"))
    );
}

#[test]
fn its_output_is_shown_as_clean_text() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let provider = ExternalProvider::new(
        spec(
            "Echo esc",
            &["pdf"],
            vec![
                echo(),
                "--esc".into(),
                "hello".into(),
                PATH_PLACEHOLDER.into(),
            ],
        ),
        Duration::from_secs(10),
    );
    let document = provider.load(&path, &sniff_for(&path)).unwrap();
    let Document::Lines(doc) = document else {
        panic!()
    };
    let text: String = doc
        .lines
        .iter()
        .map(|l| l.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("hello"), "{text}");
    assert!(text.contains("red"), "{text}");
    for bad in ['\u{1b}', '\u{7}', '\u{8}', '\u{0}'] {
        assert!(!text.contains(bad), "control char {bad:?} in {text:?}");
    }
    assert!(!doc.truncated);
}

#[test]
fn a_failing_provider_is_a_view_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let provider = ExternalProvider::new(
        spec(
            "Echo fail",
            &["pdf"],
            vec![echo(), "--exit".into(), "2".into(), PATH_PLACEHOLDER.into()],
        ),
        Duration::from_secs(10),
    );
    let Err(ViewError::Io(reason)) = provider.load(&path, &sniff_for(&path)) else {
        panic!()
    };
    assert!(reason.contains("provider"), "{reason}");
}

#[test]
fn opening_can_be_cancelled_while_the_provider_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.pdf");
    let mut registry = Registry::default();
    registry.register(Box::new(ExternalProvider::new(
        spec(
            "Echo slow",
            &["pdf"],
            vec![
                echo(),
                "--sleep-ms".into(),
                "10000".into(),
                PATH_PLACEHOLDER.into(),
            ],
        ),
        Duration::from_secs(30),
    )));
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Relaxed);
    });
    let start = Instant::now();
    let result = open_document_cancellable(
        &registry,
        &path,
        Some(ViewerKind::Custom("Echo slow")),
        None,
        &cancel,
    );
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    // Cancelled: an error, and no fallback to the next viewer (the user asked to stop).
    assert!(result.is_err(), "{:?}", result.map(|o| o.kind));
}

#[test]
fn a_failing_provider_falls_back_and_says_why() {
    // C28: the user must see why the pack view was not used, not just a different viewer.
    let tmp = tempfile::tempdir().unwrap();
    let path = file(tmp.path(), "a.txt");
    let mut registry = Registry::default();
    registry.register(Box::new(ExternalProvider::new(
        spec(
            "Echo fail",
            &["txt"],
            vec![echo(), "--exit".into(), "2".into(), PATH_PLACEHOLDER.into()],
        ),
        Duration::from_secs(10),
    )));
    let opened = open_document_cancellable(&registry, &path, None, None, &never()).unwrap();
    assert_eq!(opened.kind, ViewerKind::Text, "the next candidate is used");
    let (failed, reason) = opened.fallback.expect("the failure is reported");
    assert_eq!(failed, ViewerKind::Custom("Echo fail"));
    assert!(reason.contains("exit 2"), "{reason}");
    // A normal open has no fallback.
    let plain =
        open_document_cancellable(&Registry::default(), &path, None, None, &never()).unwrap();
    assert_eq!(plain.fallback, None);
}
