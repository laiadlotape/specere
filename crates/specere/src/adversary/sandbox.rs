//! FR-EQ-024 — sandbox for LLM-generated test code.
//!
//! Three modes:
//! - `none` — run via `/bin/sh -c`, no isolation (only safe with trusted
//!   providers or during tests with MockProvider).
//! - `rlimit` — wrap in `bash -c` with `ulimit -t 30 -v 524288`, still in
//!   the current process tree but with hard CPU + address-space caps.
//! - `bubblewrap` — `bwrap` with `--unshare-all --die-with-parent`
//!   read-only mount of the repo, writable tmpfs /scratch, no network.
//!
//! All modes enforce a 30-second wall-clock timeout via SIGKILL.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    None,
    Rlimit,
    Bubblewrap,
}

impl Mode {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "none" => Ok(Mode::None),
            "rlimit" => Ok(Mode::Rlimit),
            "bubblewrap" | "bwrap" => Ok(Mode::Bubblewrap),
            other => Err(anyhow::anyhow!(
                "--sandbox: unknown mode {other:?} (expected none|rlimit|bubblewrap)"
            )),
        }
    }
}

pub struct RunOutcome {
    pub status: i32,
    #[allow(dead_code)]
    pub stdout: String,
    #[allow(dead_code)]
    pub stderr: String,
    pub timed_out: bool,
}

impl RunOutcome {
    pub fn failed(&self) -> bool {
        !self.timed_out && self.status != 0
    }
}

/// Execute `script` in the selected sandbox. `repo` becomes the working
/// dir (read-only in bwrap mode). `scratch` is a writable tmpdir, mounted
/// at /scratch inside bwrap. 30 s hard wall-clock.
pub fn run(
    mode: Mode,
    repo: &Path,
    scratch: &Path,
    script: &str,
    timeout: Duration,
) -> Result<RunOutcome> {
    std::fs::create_dir_all(scratch).with_context(|| format!("mkdir -p {}", scratch.display()))?;
    let script_path = scratch.join("_adversary_exec.sh");
    std::fs::write(&script_path, script)
        .with_context(|| format!("write {}", script_path.display()))?;

    let mut cmd = match mode {
        Mode::None => {
            let mut c = Command::new("sh");
            c.arg(&script_path);
            c
        }
        Mode::Rlimit => {
            // ulimit -t sets CPU seconds cap; -v sets virtual memory KB.
            let wrapped = format!(
                "ulimit -t 30 -v 524288 2>/dev/null || true\nexec sh {}",
                shell_escape(&script_path)
            );
            let mut c = Command::new("bash");
            c.arg("-c").arg(wrapped);
            c
        }
        Mode::Bubblewrap => build_bwrap(repo, scratch, &script_path),
    };
    cmd.current_dir(repo);

    run_with_timeout(cmd, timeout)
}

fn build_bwrap(repo: &Path, scratch: &Path, script: &Path) -> Command {
    let mut c = Command::new("bwrap");
    c.arg("--unshare-all")
        .arg("--die-with-parent")
        .arg("--ro-bind")
        .arg("/usr")
        .arg("/usr")
        .arg("--ro-bind")
        .arg("/lib")
        .arg("/lib")
        .arg("--ro-bind")
        .arg("/lib64")
        .arg("/lib64")
        .arg("--ro-bind")
        .arg("/bin")
        .arg("/bin")
        .arg("--ro-bind")
        .arg("/etc/alternatives")
        .arg("/etc/alternatives")
        .arg("--proc")
        .arg("/proc")
        .arg("--dev")
        .arg("/dev")
        .arg("--tmpfs")
        .arg("/tmp")
        .arg("--ro-bind")
        .arg(repo)
        .arg(repo)
        .arg("--bind")
        .arg(scratch)
        .arg("/scratch")
        .arg("--chdir")
        .arg(repo)
        .arg("sh")
        .arg(script);
    c
}

fn shell_escape(p: &Path) -> String {
    let s = p.to_string_lossy();
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<RunOutcome> {
    use std::io::Read;
    use std::process::Stdio;

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("spawn sandbox process")?;

    let start = std::time::Instant::now();
    let mut timed_out = false;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = so.read_to_string(&mut stdout);
    }
    if let Some(mut se) = child.stderr.take() {
        let _ = se.read_to_string(&mut stderr);
    }
    let status = if timed_out {
        124
    } else {
        child.wait().ok().and_then(|s| s.code()).unwrap_or(-1)
    };
    Ok(RunOutcome {
        status,
        stdout,
        stderr,
        timed_out,
    })
}

pub fn default_scratch(repo: &Path) -> PathBuf {
    repo.join(".specere/adversary-scratch")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn mode_parse_roundtrip() {
        assert_eq!(Mode::parse("none").unwrap(), Mode::None);
        assert_eq!(Mode::parse("rlimit").unwrap(), Mode::Rlimit);
        assert_eq!(Mode::parse("bubblewrap").unwrap(), Mode::Bubblewrap);
        assert_eq!(Mode::parse("bwrap").unwrap(), Mode::Bubblewrap);
        assert!(Mode::parse("network").is_err());
    }

    #[test]
    fn none_mode_runs_trivial_script() {
        let tmp = TempDir::new().unwrap();
        let out = run(
            Mode::None,
            tmp.path(),
            &tmp.path().join("scratch"),
            "echo hello",
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(out.status, 0);
        assert!(out.stdout.contains("hello"));
        assert!(!out.timed_out);
    }

    #[test]
    fn none_mode_captures_nonzero_exit() {
        let tmp = TempDir::new().unwrap();
        let out = run(
            Mode::None,
            tmp.path(),
            &tmp.path().join("scratch"),
            "exit 7",
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(out.status, 7);
        assert!(out.failed());
    }

    #[test]
    fn timeout_enforced() {
        let tmp = TempDir::new().unwrap();
        let out = run(
            Mode::None,
            tmp.path(),
            &tmp.path().join("scratch"),
            "sleep 5",
            Duration::from_millis(500),
        )
        .unwrap();
        assert!(out.timed_out, "expected timeout, got status {}", out.status);
    }
}
