//! Test fixture for external providers (tests/external_provider.rs). Not part of FDS.
//!
//! Prints each argument after the options on its own line (exactly as received), then behaves
//! as the options say:
//!   --sleep-ms N   sleep N ms before printing
//!   --stderr TEXT  write TEXT to standard error
//!   --bytes N      also write N bytes of 'x' to standard output
//!   --esc          also write terminal escape sequences and control characters
//!   --exit N       exit with status N (default 0)

use std::io::Write;

fn main() {
    let mut args = std::env::args_os().skip(1).peekable();
    let (mut sleep, mut exit, mut bytes, mut esc) = (0_u64, 0_i32, 0_usize, false);
    let mut stderr_text = None;
    let number = |v: Option<std::ffi::OsString>| -> u64 {
        v.and_then(|s| s.into_string().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };
    while let Some(arg) = args.peek() {
        match arg.to_str() {
            Some("--sleep-ms") => {
                args.next();
                sleep = number(args.next());
            }
            Some("--exit") => {
                args.next();
                exit = number(args.next()) as i32;
            }
            Some("--bytes") => {
                args.next();
                bytes = number(args.next()) as usize;
            }
            Some("--stderr") => {
                args.next();
                stderr_text = args.next();
            }
            Some("--esc") => {
                args.next();
                esc = true;
            }
            _ => break,
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(sleep));
    let mut out = std::io::stdout().lock();
    for arg in args {
        let _ = writeln!(out, "{}", arg.to_string_lossy());
    }
    if esc {
        let _ = out.write_all(b"\x1b[2J\x1b[31mred\x1b]0;title\x07\x08\x00end\n");
    }
    // Write in chunks so a reader that stops early makes us block or fail, like real tools.
    let chunk = vec![b'x'; 64 * 1024];
    let mut left = bytes;
    while left > 0 {
        let n = left.min(chunk.len());
        if out.write_all(&chunk[..n]).is_err() {
            break;
        }
        left -= n;
    }
    let _ = out.flush();
    if let Some(text) = stderr_text {
        let _ = writeln!(std::io::stderr(), "{}", text.to_string_lossy());
    }
    std::process::exit(exit);
}
