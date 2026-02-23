// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
//! PTY spawning and I/O thread management.
//!
//! This module handles spawning processes in PTYs and reading their output
//! on a background thread.

use std::io::{Read, Write};
use std::path::Path;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{unbounded, Receiver, Sender};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::event::TerminalEvent;
// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
use crate::pty_wakeup::PtyWakeup;

/// Handle to a PTY process and its I/O thread.
pub struct PtyHandle {
    /// Writer to send input to PTY stdin.
    master: Box<dyn MasterPty + Send>,
    /// Writer instance for sending input to the PTY.
    /// Taken from master once at creation time.
    writer: Box<dyn Write + Send>,
    /// The child process handle.
    child: Box<dyn Child + Send + Sync>,
    /// Receiver for terminal events from the reader thread.
    event_rx: Receiver<TerminalEvent>,
    /// Handle to the reader thread (for cleanup on drop).
    reader_thread: Option<JoinHandle<()>>,
    /// Sender used by the reader thread (kept to detect shutdown).
    #[allow(dead_code)]
    event_tx: Sender<TerminalEvent>,
}

impl PtyHandle {
    /// Spawns a command in a new PTY.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command to run (e.g., "/bin/zsh")
    /// * `args` - Arguments to pass to the command
    /// * `cwd` - Working directory for the command
    /// * `rows` - Number of terminal rows
    /// * `cols` - Number of terminal columns
    /// * `login_shell` - If true, spawns as a login shell (sources full profile chain)
    ///
    /// # Returns
    ///
    /// A `PtyHandle` that can be used to interact with the PTY.
    ///
    /// # Login Shell Behavior
    ///
    /// When `login_shell` is true, the shell is spawned as a login shell by using
    /// `portable-pty`'s `new_default_prog()` which:
    /// - Reads the user's shell from the passwd database (via `getpwuid`)
    /// - Sets `argv[0]` to `-{shell_basename}` for login shell behavior
    ///
    /// This ensures the full profile chain is sourced (`~/.zprofile`, `~/.zshrc`, etc.),
    /// matching the behavior of standalone terminal emulators like Terminal.app.
    // Chunk: docs/chunks/terminal_shell_env - Login shell spawning for full environment
    pub fn spawn(
        cmd: &str,
        args: &[&str],
        cwd: &Path,
        rows: u16,
        cols: u16,
        login_shell: bool,
    ) -> std::io::Result<Self> {
        let pty_system = native_pty_system();

        // Create PTY with specified size
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Build the command
        // When login_shell is true, use new_default_prog() which spawns the user's
        // shell as a login shell (argv[0] = "-zsh" etc.), ensuring the full profile
        // chain is sourced. Otherwise, use the explicit command.
        let mut cmd_builder = if login_shell {
            CommandBuilder::new_default_prog()
        } else {
            let mut builder = CommandBuilder::new(cmd);
            builder.args(args);
            builder
        };
        cmd_builder.cwd(cwd);

        // Set up environment
        cmd_builder.env("TERM", "xterm-256color");
        cmd_builder.env("COLORTERM", "truecolor");

        // Spawn the child process
        let child = pair
            .slave
            .spawn_command(cmd_builder)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Create channel for events
        let (event_tx, event_rx) = unbounded();

        // Get a reader for the PTY output
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Take the writer once at creation time
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Spawn reader thread
        let tx = event_tx.clone();
        let reader_thread = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF - PTY closed
                        break;
                    }
                    Ok(n) => {
                        // Send output to main thread
                        if tx.send(TerminalEvent::PtyOutput(buf[..n].to_vec())).is_err() {
                            // Channel closed, main thread dropped
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(TerminalEvent::PtyError(e));
                        break;
                    }
                }
            }
        });

        Ok(PtyHandle {
            master: pair.master,
            writer,
            child,
            event_rx,
            reader_thread: Some(reader_thread),
            event_tx,
        })
    }

    // Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
    // Chunk: docs/chunks/terminal_shell_env - Login shell spawning for full environment
    /// Spawns a command in a new PTY with run-loop wakeup support.
    ///
    /// Same as `spawn()`, but signals `wakeup` whenever PTY output arrives,
    /// allowing the main thread to poll and render promptly.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command to run (e.g., "/bin/zsh")
    /// * `args` - Arguments to pass to the command
    /// * `cwd` - Working directory for the command
    /// * `rows` - Number of terminal rows
    /// * `cols` - Number of terminal columns
    /// * `wakeup` - Handle to signal the main thread when PTY data arrives
    /// * `login_shell` - If true, spawns as a login shell (sources full profile chain)
    ///
    /// # Returns
    ///
    /// A `PtyHandle` that can be used to interact with the PTY.
    ///
    /// # Login Shell Behavior
    ///
    /// See `spawn()` for details on login shell behavior.
    pub fn spawn_with_wakeup(
        cmd: &str,
        args: &[&str],
        cwd: &Path,
        rows: u16,
        cols: u16,
        wakeup: PtyWakeup,
        login_shell: bool,
    ) -> std::io::Result<Self> {
        let pty_system = native_pty_system();

        // Create PTY with specified size
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Build the command
        // When login_shell is true, use new_default_prog() which spawns the user's
        // shell as a login shell (argv[0] = "-zsh" etc.), ensuring the full profile
        // chain is sourced. Otherwise, use the explicit command.
        let mut cmd_builder = if login_shell {
            CommandBuilder::new_default_prog()
        } else {
            let mut builder = CommandBuilder::new(cmd);
            builder.args(args);
            builder
        };
        cmd_builder.cwd(cwd);

        // Set up environment
        cmd_builder.env("TERM", "xterm-256color");
        cmd_builder.env("COLORTERM", "truecolor");

        // Spawn the child process
        let child = pair
            .slave
            .spawn_command(cmd_builder)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Create channel for events
        let (event_tx, event_rx) = unbounded();

        // Get a reader for the PTY output
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Take the writer once at creation time
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Spawn reader thread with wakeup support
        let tx = event_tx.clone();
        let reader_thread = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF - PTY closed
                        break;
                    }
                    Ok(n) => {
                        // Send output to main thread
                        if tx.send(TerminalEvent::PtyOutput(buf[..n].to_vec())).is_err() {
                            // Channel closed, main thread dropped
                            break;
                        }
                        // Signal main thread to wake and poll
                        wakeup.signal();
                    }
                    Err(e) => {
                        let _ = tx.send(TerminalEvent::PtyError(e));
                        break;
                    }
                }
            }
        });

        Ok(PtyHandle {
            master: pair.master,
            writer,
            child,
            event_rx,
            reader_thread: Some(reader_thread),
            event_tx,
        })
    }

    /// Writes data to the PTY stdin.
    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Resizes the PTY to the given dimensions.
    pub fn resize(&self, rows: u16, cols: u16) -> std::io::Result<()> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        self.master
            .resize(size)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    /// Returns a reference to the event receiver.
    #[allow(dead_code)]
    pub fn events(&self) -> &Receiver<TerminalEvent> {
        &self.event_rx
    }

    /// Tries to receive an event without blocking.
    pub fn try_recv(&self) -> Option<TerminalEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Checks if the child process has exited.
    ///
    /// Returns `Some(exit_code)` if the process has exited, `None` otherwise.
    pub fn try_wait(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code() as i32),
            Ok(None) => None,
            Err(_) => Some(-1), // Error checking status, assume dead
        }
    }

    /// Kills the child process.
    ///
    /// This sends SIGKILL to immediately terminate the process.
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child
            .kill()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    /// Returns the process ID of the child process, if available.
    ///
    /// This is used for sending signals (e.g., SIGTERM for graceful shutdown).
    pub fn process_id(&self) -> Option<u32> {
        self.child.process_id()
    }
}

impl Drop for PtyHandle {
    fn drop(&mut self) {
        // Kill the process if it's still running to ensure the reader thread
        // will hit EOF or an error and exit.
        let _ = self.child.kill();

        // The reader thread will exit when it hits EOF or an error
        // after the PTY is closed. We don't join it to avoid blocking.
        // The thread will be detached and cleaned up by the OS.
        //
        // Note: We explicitly don't join here because the reader thread
        // may be blocked on read() and killing the process may not
        // immediately unblock it on all platforms.
        self.reader_thread.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_spawn_echo() {
        // Spawn a simple echo command (explicit command, not login shell)
        let handle = PtyHandle::spawn(
            "echo",
            &["hello"],
            Path::new("/tmp"),
            24,
            80,
            false, // Not a login shell - explicit command
        );

        assert!(handle.is_ok(), "Failed to spawn PTY: {:?}", handle.err());
        let handle = handle.unwrap();

        // Wait a bit for output
        std::thread::sleep(Duration::from_millis(100));

        // Check we got some output
        let mut found_hello = false;
        while let Some(event) = handle.try_recv() {
            if let TerminalEvent::PtyOutput(data) = event {
                let output = String::from_utf8_lossy(&data);
                if output.contains("hello") {
                    found_hello = true;
                }
            }
        }

        assert!(found_hello, "Expected to find 'hello' in PTY output");
    }

    #[test]
    fn test_spawn_exit_code() {
        // Spawn a command that exits immediately with code 0 (explicit command, not login shell)
        let mut handle = PtyHandle::spawn(
            "true",
            &[],
            Path::new("/tmp"),
            24,
            80,
            false, // Not a login shell - explicit command
        ).expect("Failed to spawn PTY");

        // Wait for exit
        std::thread::sleep(Duration::from_millis(100));

        // Check exit code
        let exit_code = handle.try_wait();
        assert_eq!(exit_code, Some(0));
    }

    // Chunk: docs/chunks/terminal_shell_env - Login shell integration test
    #[test]
    fn test_spawn_login_shell() {
        // Spawn a login shell using new_default_prog()
        // This tests that login_shell=true correctly uses the passwd-detected
        // shell and sets argv[0] to "-{shell}" for login shell behavior.
        let handle = PtyHandle::spawn(
            "", // Ignored when login_shell=true
            &[],
            Path::new("/tmp"),
            24,
            80,
            true, // Login shell
        );

        assert!(handle.is_ok(), "Failed to spawn login shell: {:?}", handle.err());
        let mut handle = handle.unwrap();

        // Wait for the shell to start
        std::thread::sleep(Duration::from_millis(200));

        // Drain any initial output (prompt, etc.)
        while handle.try_recv().is_some() {}

        // Send "echo $0" to the shell to verify it reports as a login shell
        // (should show "-zsh" or "-bash" etc., with the leading dash)
        handle.write(b"echo $0\n").expect("Failed to write to PTY");

        // Wait for output with multiple polling attempts
        let mut output = String::new();
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(100));
            while let Some(event) = handle.try_recv() {
                if let TerminalEvent::PtyOutput(data) = event {
                    output.push_str(&String::from_utf8_lossy(&data));
                }
            }
            // Check if we have the expected output yet
            if output.contains('-') {
                break;
            }
        }

        // The shell's $0 should start with a dash, indicating it's a login shell
        // (e.g., "-zsh", "-bash", "-fish")
        // We check that the output contains a line starting with "-"
        //
        // Note: The output may include the echoed command and shell prompt, so we
        // look for any line that is just a dash followed by a shell name.
        let has_login_indicator = output.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('-') && trimmed.len() > 1 && !trimmed.contains(' ')
        });

        assert!(
            has_login_indicator,
            "Expected login shell indicator (argv[0] starting with '-') in output: {}",
            output
        );
    }
}
