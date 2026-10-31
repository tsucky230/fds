//! Terminal setup / teardown (design prompt §19 / R-1300), kept apart from business logic.
//!
//! - Normal exit and error return: restored by `TerminalGuard`'s Drop.
//! - Panic: the panic hook installed by `ratatui::try_init` restores before printing the message.
//! - SIGINT / SIGTERM / SIGHUP (Unix): `ExitSignal` sets a flag, the event loop exits, Drop restores.
//!   A watcher thread forces the exit if the loop is stuck (see `ExitSignal::register`).
//! - Ctrl+Z (Unix): `suspend` restores, stops the process, and re-initializes after resume.
//! - Ctrl+C while an external program has the terminal: for that program only (`ExternalProgram`).

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::DefaultTerminal;

/// Raw mode + alternate screen while alive. Always restored on Drop.
pub struct TerminalGuard {
    pub terminal: DefaultTerminal,
}

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        let terminal = ratatui::try_init()?;
        Ok(Self { terminal })
    }

    /// Unix: restore the terminal, stop with SIGTSTP, and re-initialize after SIGCONT.
    #[cfg(unix)]
    pub fn suspend(&mut self) -> io::Result<()> {
        ratatui::restore();
        signal_hook::low_level::raise(signal_hook::consts::SIGTSTP)?;
        // We get here after SIGCONT. The panic hook is already installed, so only restore the modes.
        use ratatui::crossterm::{execute, terminal};
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), terminal::EnterAlternateScreen)?;
        self.terminal.clear()?;
        Ok(())
    }

    /// Runs `f` (an external program) with the terminal restored, then re-enters raw mode and the
    /// alternate screen and clears so the next frame is fully redrawn (P3-15, R-1200-4).
    /// The terminal is re-initialized even if `f` fails.
    pub fn run_external<T>(&mut self, f: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
        use ratatui::crossterm::{execute, terminal};
        ratatui::restore();
        let result = {
            let _external = ExternalProgram::start();
            f()
        };
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), terminal::EnterAlternateScreen)?;
        self.terminal.clear()?;
        result
    }

    /// Windows has no suspend; do nothing.
    #[cfg(not(unix))]
    pub fn suspend(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

/// Whether a signal asking us to exit has been received.
#[derive(Clone, Default)]
pub struct ExitSignal(Arc<AtomicBool>);

impl ExitSignal {
    /// Unix: SIGINT / SIGTERM / SIGHUP set the flag, and the event loop exits normally.
    ///
    /// A watcher thread also enforces the exit: if the loop has not ended within
    /// `FORCED_EXIT_GRACE`, it restores the terminal and exits the process. This is needed because
    /// crossterm 0.29 can spin forever inside `event::poll` once the tty is gone (e.g. SSH
    /// disconnect), so the loop never gets to check the flag.
    ///
    /// Nothing is registered on Windows (in raw mode Ctrl+C arrives as a key and becomes Quit).
    pub fn register() -> io::Result<Self> {
        let signal = Self::default();
        #[cfg(unix)]
        {
            use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
            let mut signals = signal_hook::iterator::Signals::new([SIGINT, SIGTERM, SIGHUP])?;
            let flag = Arc::clone(&signal.0);
            std::thread::spawn(move || {
                for sig in signals.forever() {
                    if !signal_ends_fds(sig, EXTERNAL_RUNNING.load(Ordering::Relaxed)) {
                        continue;
                    }
                    flag.store(true, Ordering::Relaxed);
                    std::thread::sleep(FORCED_EXIT_GRACE);
                    // Still running: the main loop is stuck. Restore and leave.
                    ratatui::restore();
                    std::process::exit(128 + sig);
                }
            });
        }
        Ok(signal)
    }

    pub fn raised(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Whether a signal ends FDS. While an external program has the terminal, Ctrl+C is meant for that
/// program, so SIGINT does not end FDS; SIGTERM and SIGHUP always do (R-412).
#[cfg(unix)]
pub fn signal_ends_fds(signal: i32, external_running: bool) -> bool {
    signal != signal_hook::consts::SIGINT || !external_running
}

/// An external program has the terminal, which is out of raw mode, so Ctrl+C reaches every process
/// on it, FDS included.
#[cfg(unix)]
static EXTERNAL_RUNNING: AtomicBool = AtomicBool::new(false);

/// While alive, Ctrl+C does not end FDS. The program gets it as usual.
struct ExternalProgram;

#[cfg(unix)]
impl ExternalProgram {
    fn start() -> Self {
        EXTERNAL_RUNNING.store(true, Ordering::Relaxed);
        Self
    }
}

#[cfg(unix)]
impl Drop for ExternalProgram {
    fn drop(&mut self) {
        EXTERNAL_RUNNING.store(false, Ordering::Relaxed);
    }
}

/// Handlers are not inherited, unlike `SetConsoleCtrlHandler(NULL, TRUE)`, which would make the
/// program ignore Ctrl+C too.
#[cfg(windows)]
unsafe extern "system" fn ignore_ctrl_c(event: u32) -> windows::Win32::Foundation::BOOL {
    use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
    (event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT).into()
}

#[cfg(windows)]
impl ExternalProgram {
    fn start() -> Self {
        use windows::Win32::System::Console::SetConsoleCtrlHandler;
        // SAFETY: the handler is a plain function that touches nothing but its argument.
        let _ = unsafe { SetConsoleCtrlHandler(Some(ignore_ctrl_c), true) };
        Self
    }
}

#[cfg(windows)]
impl Drop for ExternalProgram {
    fn drop(&mut self) {
        use windows::Win32::System::Console::SetConsoleCtrlHandler;
        // SAFETY: as in `start`.
        let _ = unsafe { SetConsoleCtrlHandler(Some(ignore_ctrl_c), false) };
    }
}

/// How long the watcher waits for a normal exit before forcing one.
#[cfg(unix)]
const FORCED_EXIT_GRACE: std::time::Duration = std::time::Duration::from_millis(500);
