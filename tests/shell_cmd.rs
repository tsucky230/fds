//! Turning a typed line into a shell command line (R-412). Both shells are checked on every OS as
//! pure text; the tests at the end run the real shell of the OS.
//!
//! Uses the fixture program `examples/provider_echo.rs`, which `cargo test` builds next to the
//! test binaries.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use fds::open::Launch;
use fds::shell_cmd::{
    CMD_LINE_LIMIT, CommandError, ShellKind, check_folder, expand, parse_cd, program_prefill,
    resolve_cd, runnable_in_shell, shell_command,
};

use ShellKind::{Cmd, Posix};

fn names(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn one(template: &str, name: &str, kind: ShellKind) -> Result<String, CommandError> {
    expand(template, Some(OsStr::new(name)), &names(&[name]), kind)
}

#[test]
fn posix_names_never_become_syntax() {
    for (name, quoted) in [
        ("a.txt", "'a.txt'"),
        ("a b.txt", "'a b.txt'"),
        ("it's", r"'it'\''s'"),
        ("'", r"''\'''"),
        ("$HOME", "'$HOME'"),
        ("`id`", "'`id`'"),
        ("$(id)", "'$(id)'"),
        (";rm x", "';rm x'"),
        ("&x", "'&x'"),
        ("|x", "'|x'"),
        ("*", "'*'"),
        ("\"q\"", "'\"q\"'"),
        ("back\\slash", "'back\\slash'"),
        ("line\nbreak", "'line\nbreak'"),
        ("%PATH%", "'%PATH%'"),
        ("日本語.txt", "'日本語.txt'"),
        ("😀", "'😀'"),
    ] {
        assert_eq!(
            one("ls {file}", name, Posix),
            Ok(format!("ls {quoted}")),
            "{name:?}"
        );
    }
}

#[test]
fn cmd_names_never_become_syntax() {
    for (name, quoted) in [
        ("a.txt", "\"a.txt\""),
        ("a b.txt", "\"a b.txt\""),
        ("it's", "\"it's\""),
        ("&x", "\"&x\""),
        ("|x", "\"|x\""),
        ("^x", "\"^x\""),
        ("!x", "\"!x\""),
        (";x", "\";x\""),
        ("$x", "\"$x\""),
        ("(x)", "\"(x)\""),
        ("日本語.txt", "\"日本語.txt\""),
        ("😀", "\"😀\""),
    ] {
        assert_eq!(
            one("dir {file}", name, Cmd),
            Ok(format!("dir {quoted}")),
            "{name:?}"
        );
    }
}

#[test]
fn names_cmd_cannot_quote_are_refused_not_run() {
    for name in ["100%.txt", "%PATH%", "a%b", "a\"b", "a\nb", "a\rb"] {
        assert_eq!(
            one("dir {file}", name, Cmd),
            Err(CommandError::UnsafeName { name: name.into() }),
            "{name:?}"
        );
    }
}

#[test]
fn names_starting_with_a_dash_are_not_options() {
    for name in ["-rf", "--help", "-"] {
        assert_eq!(
            one("rm {file}", name, Posix),
            Ok(format!("rm './{name}'")),
            "{name:?}"
        );
        assert_eq!(
            one("del {file}", name, Cmd),
            Ok(format!("del \".\\{name}\"")),
            "{name:?}"
        );
    }
    // Not at the start of the name: unchanged.
    assert_eq!(one("rm {file}", "a-b", Posix), Ok("rm 'a-b'".into()));
}

#[test]
fn files_are_every_target_in_order() {
    let targets = names(&["a", "b c", "-d"]);
    assert_eq!(
        expand("tar cf x.tar {files}", None, &targets, Posix),
        Ok("tar cf x.tar 'a' 'b c' './-d'".into())
    );
    assert_eq!(
        expand("tar cf x.tar {files}", None, &targets, Cmd),
        Ok("tar cf x.tar \"a\" \"b c\" \".\\-d\"".into())
    );
    assert_eq!(
        expand("cp {files} {files} ..", None, &names(&["a"]), Posix),
        Ok("cp 'a' 'a' ..".into())
    );
}

#[test]
fn file_is_the_cursor_even_when_files_are_marked() {
    assert_eq!(
        expand(
            "diff {file} {files}",
            Some(OsStr::new("c")),
            &names(&["a", "b"]),
            Posix
        ),
        Ok("diff 'c' 'a' 'b'".into())
    );
}

#[test]
fn file_may_be_part_of_a_word() {
    assert_eq!(
        one("cp {file} {file}.bak", "a b", Posix),
        Ok("cp 'a b' 'a b'.bak".into())
    );
    assert_eq!(
        one("copy {file} {file}.bak", "a b", Cmd),
        Ok("copy \"a b\" \"a b\".bak".into())
    );
    assert_eq!(one("x --in={file}", "a", Posix), Ok("x --in='a'".into()));
}

#[test]
fn a_line_without_placeholders_is_left_exactly_as_typed() {
    for line in [
        "git status",
        "grep foo *.log | sort > out.txt",
        "  ls   -l  ",
        "echo %PATH% $HOME ^& 'q' \"r\"",
        "echo it's",
    ] {
        for kind in [Posix, Cmd] {
            // Even with a name that could not be quoted: no placeholder uses it.
            assert_eq!(
                expand(line, Some(OsStr::new("a%\"b")), &names(&["a%\"b"]), kind),
                Ok(line.to_string()),
                "{line:?}"
            );
            assert_eq!(expand(line, None, &[], kind), Ok(line.to_string()));
        }
    }
}

#[test]
fn other_braces_stay() {
    for (template, expected) in [
        ("awk '{print $1}' {file}", "awk '{print $1}' 'a'"),
        ("echo {File} {file}", "echo {File} 'a'"),
        ("echo { file } {file}", "echo { file } 'a'"),
        ("echo {filez} {file}", "echo {filez} 'a'"),
        ("echo {} {file}", "echo {} 'a'"),
        ("echo {file", "echo {file"),
    ] {
        assert_eq!(one(template, "a", Posix), Ok(expected.into()), "{template}");
    }
}

#[test]
fn placeholders_inside_typed_quotes_are_refused() {
    for (template, kind) in [
        ("echo \"{file}\"", Posix),
        ("echo '{file}'", Posix),
        ("echo \"a {file} b\"", Posix),
        ("echo \"{files}\"", Posix),
        ("echo 'x' '{files}", Posix),
        ("echo it's {file}", Posix),
        ("echo \"\\\"{file}\"", Posix),
        ("echo \"{file}\"", Cmd),
        ("echo \"a {files}\"", Cmd),
        ("echo \"unclosed {file}", Cmd),
    ] {
        assert_eq!(
            one(template, "a", kind),
            Err(CommandError::QuotedPlaceholder),
            "{template}"
        );
    }
}

#[test]
fn placeholders_after_closed_quotes_are_expanded() {
    for (template, kind, expected) in [
        ("echo \"hello\" {file}", Posix, "echo \"hello\" 'a'"),
        ("echo 'hello' {file}", Posix, "echo 'hello' 'a'"),
        ("echo 'it''s' {file}", Posix, "echo 'it''s' 'a'"),
        ("echo \\\" {file}", Posix, "echo \\\" 'a'"),
        ("echo \\' {file}", Posix, "echo \\' 'a'"),
        ("echo \"\\\"\" {file}", Posix, "echo \"\\\"\" 'a'"),
        ("echo '\\' {file}", Posix, "echo '\\' 'a'"),
        ("echo \"hello\" {file}", Cmd, "echo \"hello\" \"a\""),
        // ' is not a quote for cmd.exe.
        ("echo it's {file}", Cmd, "echo it's \"a\""),
        ("echo ^\" {file}", Cmd, "echo ^\" \"a\""),
        // \ does not escape for cmd.exe.
        ("echo \"a\\\" {file}", Cmd, "echo \"a\\\" \"a\""),
    ] {
        assert_eq!(one(template, "a", kind), Ok(expected.into()), "{template}");
    }
}

#[test]
fn files_must_be_a_word() {
    for template in [
        "x{files}",
        "x {files}.bak",
        "x --in={files}",
        "x a{files}b",
        "x {files}{files}",
    ] {
        assert_eq!(
            one(template, "a", Posix),
            Err(CommandError::FilesNotAWord),
            "{template}"
        );
    }
    for (template, expected) in [
        ("cat {files}|sort", "cat 'a'|sort"),
        ("(ls {files})", "(ls 'a')"),
        ("ls {files};ls", "ls 'a';ls"),
        ("ls {files}>x", "ls 'a'>x"),
        ("ls\t{files}", "ls\t'a'"),
        ("ls {files}&", "ls 'a'&"),
    ] {
        assert_eq!(one(template, "a", Posix), Ok(expected.into()), "{template}");
    }
}

#[test]
fn a_line_needs_a_command() {
    for template in [
        "",
        "   ",
        "\t",
        " {files}",
        " {file}",
        "{file}",
        "{files}",
        "{file} x",
        "\t{files} x",
    ] {
        for kind in [Posix, Cmd] {
            assert_eq!(
                one(template, "a", kind),
                Err(CommandError::NoCommand),
                "{template:?}"
            );
        }
    }
}

#[test]
fn a_placeholder_needs_a_target() {
    for (template, cursor, targets) in [
        ("ls {file}", None, vec![]),
        ("ls {files}", None, vec![]),
        ("ls {file}", None, names(&["a"])),
        ("ls {files}", Some(OsStr::new("a")), vec![]),
    ] {
        assert_eq!(
            expand(template, cursor, &targets, Posix),
            Err(CommandError::NoTarget),
            "{template}"
        );
    }
}

#[cfg(unix)]
fn not_utf8() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(vec![b'a', 0xFF, b'b'])
}

#[cfg(windows)]
fn not_utf8() -> OsString {
    use std::os::windows::ffi::OsStringExt;
    OsString::from_wide(&[0x61, 0xD800, 0x62])
}

#[test]
fn names_that_are_not_utf8_are_refused() {
    let bad = not_utf8();
    let reason = CommandError::NonUtf8 {
        name: bad.to_string_lossy().into_owned(),
    };
    for kind in [Posix, Cmd] {
        assert_eq!(
            expand(
                "ls {file}",
                Some(bad.as_os_str()),
                std::slice::from_ref(&bad),
                kind
            ),
            Err(reason.clone())
        );
        assert_eq!(
            expand(
                "ls {files}",
                None,
                &[OsString::from("ok"), bad.clone()],
                kind
            ),
            Err(reason.clone())
        );
        assert_eq!(program_prefill(bad.as_os_str(), kind), Err(reason.clone()));
    }
}

#[test]
fn cmd_lines_over_the_limit_are_refused_not_cut() {
    // `echo "` + name + `"` is 7 characters more than the name.
    let room = CMD_LINE_LIMIT - 7;
    for (name, expect_ok) in [
        ("x".repeat(room - 1), true),
        ("x".repeat(room), true),
        ("x".repeat(room + 1), false),
        ("あ".repeat(room), true),
        ("あ".repeat(room + 1), false),
        // Two UTF-16 units each.
        ("😀".repeat(room / 2), true),
        ("😀".repeat(room / 2 + 1), false),
    ] {
        let result = one("echo {file}", &name, Cmd);
        if expect_ok {
            assert!(result.is_ok(), "{} chars", name.chars().count());
        } else {
            assert_eq!(result, Err(CommandError::TooLong));
        }
    }
    let many: Vec<OsString> = (0..3000).map(|i| format!("file{i}.txt").into()).collect();
    assert_eq!(
        expand("echo {files}", None, &many, Cmd),
        Err(CommandError::TooLong)
    );
    // The limit belongs to cmd.exe.
    assert!(expand("echo {files}", None, &many, Posix).is_ok());
}

#[test]
fn program_prefills() {
    for (name, kind, expected) in [
        ("build.sh", Posix, "./build.sh "),
        ("a-b_c+d=e:f@g", Posix, "./a-b_c+d=e:f@g "),
        ("my tool.sh", Posix, "'./my tool.sh' "),
        ("it's", Posix, r"'./it'\''s' "),
        ("$x", Posix, "'./$x' "),
        ("-x", Posix, "./-x "),
        ("build.bat", Cmd, ".\\build.bat "),
        ("my tool.exe", Cmd, "\".\\my tool.exe\" "),
        ("a&b.exe", Cmd, "\".\\a&b.exe\" "),
        ("-x.exe", Cmd, ".\\-x.exe "),
    ] {
        assert_eq!(
            program_prefill(OsStr::new(name), kind),
            Ok(expected.to_string()),
            "{name}"
        );
    }
    assert_eq!(
        program_prefill(OsStr::new("a%b.exe"), Cmd),
        Err(CommandError::UnsafeName {
            name: "a%b.exe".into()
        })
    );
}

#[test]
fn cd_lines_on_posix() {
    for (line, expected) in [
        ("cd sub", Some("sub")),
        ("cd ..", Some("..")),
        ("  cd   sub  ", Some("sub")),
        ("cd /tmp", Some("/tmp")),
        ("cd 'my folder'", Some("my folder")),
        ("cd \"my folder\"", Some("my folder")),
        ("cd 'a$b;c'", Some("a$b;c")),
        ("cd", None),
        ("cd ", None),
        ("cd ''", None),
        ("cdx y", None),
        ("CD sub", None),
        ("cd a && ls", None),
        ("cd a;ls", None),
        ("cd a|b", None),
        ("cd $HOME", None),
        ("cd ~", None),
        ("cd *", None),
        ("cd my\\ folder", None),
        ("cd my folder", None),
        ("cd \"a$b\"", None),
        ("cd \"a\\b\"", None),
        ("cd -", None),
        (r"cd 'it'\''s'", None),
        ("cd 'a", None),
    ] {
        assert_eq!(parse_cd(line, Posix).as_deref(), expected, "{line:?}");
    }
}

#[test]
fn cd_lines_on_cmd() {
    for (line, expected) in [
        ("cd sub", Some("sub")),
        ("cd ..", Some("..")),
        ("CD sub", Some("sub")),
        ("Cd sub", Some("sub")),
        ("  cd   sub  ", Some("sub")),
        ("cd my folder", Some("my folder")),
        ("cd \"my folder\"", Some("my folder")),
        ("cd C:\\x", Some("C:\\x")),
        ("cd /d D:\\x", Some("D:\\x")),
        ("cd /D \"D:\\my x\"", Some("D:\\my x")),
        ("cd 'a'", Some("'a'")),
        ("cd", None),
        ("cd ", None),
        ("cd \"\"", None),
        ("cdx y", None),
        ("cd a & dir", None),
        ("cd a|b", None),
        ("cd %TEMP%", None),
        ("cd ^a", None),
        ("cd a>b", None),
        ("cd \"a", None),
    ] {
        assert_eq!(parse_cd(line, Cmd).as_deref(), expected, "{line:?}");
    }
}

#[test]
fn cd_paths_are_resolved_without_the_disk() {
    let sep = std::path::MAIN_SEPARATOR;
    let root = PathBuf::from(if cfg!(windows) { r"C:\w" } else { "/w" });
    let top = PathBuf::from(if cfg!(windows) { r"C:\" } else { "/" });
    for (path, expected) in [
        ("sub", root.join("sub")),
        ("..", top.clone()),
        ("../..", top.clone()),
        (".", root.clone()),
        ("a/../b", root.join("b")),
        ("./a/./b", root.join("a").join("b")),
        (&format!("a{sep}b")[..], root.join("a").join("b")),
    ] {
        assert_eq!(resolve_cd(&root, path), expected, "{path}");
    }
    let elsewhere = if cfg!(windows) { r"D:\x" } else { "/x" };
    assert_eq!(resolve_cd(&root, elsewhere), PathBuf::from(elsewhere));
    assert_eq!(resolve_cd(&top, ".."), top);
}

#[test]
fn cmd_refuses_unc_folders() {
    for dir in [r"\\server\share", r"\\server\share\sub", r"\\?\C:\x"] {
        assert_eq!(
            check_folder(Path::new(dir), Cmd),
            Err(CommandError::UncFolder),
            "{dir}"
        );
        assert_eq!(check_folder(Path::new(dir), Posix), Ok(()), "{dir}");
    }
    for dir in [r"C:\x", r"D:\", "/w", "//w"] {
        let result = check_folder(Path::new(dir), Cmd);
        // `//w` is UNC to Windows too, but only a `\\` prefix is looked at.
        assert_eq!(result, Ok(()), "{dir}");
    }
}

#[test]
fn which_files_x_offers_as_programs() {
    for (name, launch, kind, expected) in [
        ("a.exe", Launch::RunDirectly, Cmd, true),
        ("a.com", Launch::RunDirectly, Cmd, true),
        ("a.bat", Launch::RunThroughOs, Cmd, true),
        ("a.BAT", Launch::RunThroughOs, Cmd, true),
        ("a.cmd", Launch::RunThroughOs, Cmd, true),
        ("a.Cmd", Launch::RunThroughOs, Cmd, true),
        ("a.lnk", Launch::RunThroughOs, Cmd, false),
        ("a.url", Launch::RunThroughOs, Cmd, false),
        ("a.ps1", Launch::RunThroughOs, Cmd, false),
        ("a.txt", Launch::Document, Cmd, false),
        ("a.bat", Launch::Document, Cmd, false),
        ("build.sh", Launch::RunDirectly, Posix, true),
        ("tool", Launch::RunDirectly, Posix, true),
        ("a.bat", Launch::RunThroughOs, Posix, false),
        ("a.desktop", Launch::RunThroughOs, Posix, false),
        ("a.txt", Launch::Document, Posix, false),
    ] {
        assert_eq!(
            runnable_in_shell(name, launch, kind),
            expected,
            "{name} {launch:?}"
        );
    }
}

#[test]
fn the_current_shell_follows_the_os() {
    assert_eq!(
        ShellKind::current(),
        if cfg!(windows) { Cmd } else { Posix }
    );
}

// --- The real shell of this OS ---

fn echo() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap();
    let path = dir
        .join("examples")
        .join(format!("provider_echo{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{} is missing: run the whole `cargo test` (it builds examples) or `cargo build --example provider_echo`",
        path.display()
    );
    path
}

/// The fixture as the first word of a line, quoted for the current shell.
fn echo_word() -> String {
    let path = echo().display().to_string();
    match ShellKind::current() {
        Posix => format!("'{path}'"),
        Cmd => format!("\"{path}\""),
    }
}

fn run_line(line: &str, dir: &Path) -> std::process::Output {
    shell_command(line, ShellKind::current())
        .current_dir(dir)
        .output()
        .unwrap()
}

fn stdout_lines(output: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn names_reach_the_program_exactly_through_the_real_shell() {
    let dir = tempfile::tempdir().unwrap();
    let kind = ShellKind::current();
    let mut list = vec![
        "plain.txt",
        "a b",
        "  two  spaces  ",
        "it's",
        "$HOME",
        "`whoami`",
        "$(whoami)",
        ";echo x",
        "&echo x",
        "|echo x",
        "^x",
        "!x",
        "*",
        "?",
        "(x)",
        "<x>",
        "日本語.txt",
        "😀",
    ];
    if kind == Posix {
        list.extend(["\"q\"", "%PATH%", "back\\slash"]);
    }
    let template = format!("{} {{files}}", echo_word());
    let line = expand(&template, None, &names(&list), kind).unwrap();
    let output = run_line(&line, dir.path());
    assert!(output.status.success(), "{output:?}");
    assert_eq!(stdout_lines(&output), list);
}

#[test]
fn a_dash_name_reaches_the_program_as_a_path() {
    let dir = tempfile::tempdir().unwrap();
    let kind = ShellKind::current();
    let template = format!("{} {{file}}", echo_word());
    let line = one(&template, "--exit", kind).unwrap();
    let output = run_line(&line, dir.path());
    assert!(output.status.success(), "{output:?}");
    let sep = std::path::MAIN_SEPARATOR;
    assert_eq!(stdout_lines(&output), [format!(".{sep}--exit")]);
}

#[cfg(unix)]
#[test]
fn a_line_break_in_a_name_reaches_the_program() {
    let dir = tempfile::tempdir().unwrap();
    let template = format!("{} {{file}}", echo_word());
    let line = one(&template, "a\nb", Posix).unwrap();
    let output = run_line(&line, dir.path());
    assert_eq!(stdout_lines(&output), ["a", "b"]);
}

#[test]
fn the_real_shell_does_pipes_wildcards_and_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("one.log"), "").unwrap();
    std::fs::write(dir.path().join("two.txt"), "").unwrap();

    let wildcard = if cfg!(windows) {
        "dir /b *.log"
    } else {
        "ls *.log"
    };
    assert_eq!(
        stdout_lines(&run_line(wildcard, dir.path())),
        ["one.log"],
        "wildcard and folder"
    );

    // Not sort: on Windows it reads such a short input as UTF-16.
    let filter = if cfg!(windows) { "findstr a" } else { "grep a" };
    let pipe = format!("{} b a c | {filter}", echo_word());
    assert_eq!(stdout_lines(&run_line(&pipe, dir.path())), ["a"]);

    let redirect = format!("{} hello > out.txt", echo_word());
    run_line(&redirect, dir.path());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("out.txt"))
            .unwrap()
            .trim(),
        "hello"
    );
}

#[test]
fn the_exit_status_of_the_line_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    for code in [0, 1, 3] {
        let output = run_line(&format!("exit {code}"), dir.path());
        assert_eq!(output.status.code(), Some(code));
    }
    let output = run_line("no-such-program-for-fds", dir.path());
    assert!(!output.status.success());
}

#[test]
fn a_folder_that_is_gone_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("gone");
    assert!(
        shell_command("exit 0", ShellKind::current())
            .current_dir(&gone)
            .status()
            .is_err()
    );
}
