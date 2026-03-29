use std::ffi::CString;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

use anyhow::{Context, Result};
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::libc;
use nix::pty::{ForkptyResult, forkpty};
use nix::sys::signal::{Signal, kill};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{Pid, execvp, write};

pub struct Pty {
    master_fd: OwnedFd,
    pub child_pid: Pid,
}

impl Pty {
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &[(String, String)],
        cwd: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        let winsize = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // Safety: forkpty is safe, child process calls exec immediately
        let res = unsafe { forkpty(Some(&winsize), None) }.context("forkpty failed")?;

        match res {
            ForkptyResult::Child => {
                child_exec(command, args, env, cwd);
            }
            ForkptyResult::Parent { child, master } => {
                // Set non-blocking
                let flags = fcntl(&master, FcntlArg::F_GETFL)?;
                let flags = OFlag::from_bits_retain(flags) | OFlag::O_NONBLOCK;
                fcntl(&master, FcntlArg::F_SETFL(flags))?;

                Ok(Pty {
                    master_fd: master,
                    child_pid: child,
                })
            }
        }
    }

    pub fn raw_fd(&self) -> RawFd {
        self.master_fd.as_raw_fd()
    }

    /// Read available bytes from the PTY. Returns bytes read, or 0 on EOF/EAGAIN.
    pub fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        match nix::unistd::read(&self.master_fd, buf) {
            Ok(n) => Ok(n),
            Err(nix::errno::Errno::EAGAIN) => Ok(0),
            Err(nix::errno::Errno::EIO) => Ok(0), // Child exited
            Err(e) => Err(e.into()),
        }
    }

    /// Write bytes to the PTY master fd.
    pub fn write(&self, data: &[u8]) -> std::io::Result<()> {
        let mut offset = 0;
        while offset < data.len() {
            match write(&self.master_fd, &data[offset..]) {
                Ok(n) => offset += n,
                Err(nix::errno::Errno::EAGAIN) => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let winsize = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // Safety: valid fd and winsize struct
        let ret = unsafe { libc::ioctl(self.master_fd.as_raw_fd(), libc::TIOCSWINSZ, &winsize) };
        if ret < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    /// Check if child has exited. Returns exit code if exited.
    pub fn try_wait(&self) -> Option<i32> {
        match waitpid(self.child_pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_, code)) => Some(code),
            Ok(WaitStatus::Signaled(_, sig, _)) => Some(128 + sig as i32),
            _ => None,
        }
    }

    pub fn kill_child(&self, signal: Signal) -> Result<()> {
        kill(self.child_pid, signal)?;
        Ok(())
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = kill(self.child_pid, Signal::SIGHUP);
        let _ = waitpid(self.child_pid, Some(WaitPidFlag::WNOHANG));
    }
}

fn child_exec(command: &str, args: &[String], env: &[(String, String)], cwd: Option<&str>) -> ! {
    // Safety: we're in a forked child, single-threaded, about to exec
    unsafe {
        std::env::set_var("TERM", "xterm-256color");
        for (k, v) in env {
            std::env::set_var(k, v);
        }
    }

    if let Some(dir) = cwd {
        let _ = std::env::set_current_dir(dir);
    }

    let cmd = CString::new(command).expect("invalid command");
    let mut c_args: Vec<CString> = vec![cmd.clone()];
    for a in args {
        c_args.push(CString::new(a.as_str()).expect("invalid arg"));
    }

    let _ = execvp(&cmd, &c_args);
    std::process::exit(127);
}
