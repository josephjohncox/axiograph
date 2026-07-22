//! Bounded primitives for data and processes controlled by untrusted inputs.
//!
//! This crate is deliberately semantics-free. It centralizes the small I/O
//! interface that every Axiograph boundary uses before parsing, hashing, or
//! executing data. Opening and validation happen on one file handle; child
//! processes run in an operating-system process group/job object with bounded
//! I/O, wall time, and global concurrency.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use process_wrap::std::{ChildWrapper, CommandWrap};
use serde::de::DeserializeOwned;

pub const MAX_JSON_NESTING_DEPTH: usize = 128;
pub const MAX_CHILD_RUNTIME: Duration = Duration::from_secs(10 * 60);
pub const MAX_CONCURRENT_CHILDREN: usize = 16;
pub const MAX_CHILD_STDIN_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CHILD_STDOUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CHILD_STDERR_BYTES: usize = 1024 * 1024;
pub const DEFAULT_PLUGIN_STDIN_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_PLUGIN_STDOUT_BYTES: usize = 8 * 1024 * 1024;

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Open one regular file without following a final-component symlink and check
/// its size on the same handle that the caller will read.
///
/// Parent-directory policy belongs to the caller: workspace-confined callers
/// must separately resolve and constrain their parent path. This primitive
/// prevents the common inspect-path-then-reopen race for the file itself.
pub fn open_regular_file_bounded(path: &Path, limit: usize, label: &str) -> Result<(File, u64)> {
    if limit == 0 {
        return Err(anyhow!("{label} byte limit must be positive"));
    }

    #[cfg(unix)]
    let file = {
        use rustix::fs::{open, Mode, OFlags};

        let fd = open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .with_context(|| {
            format!(
                "failed to open {label} `{}` without following symlinks",
                path.display()
            )
        })?;
        File::from(fd)
    };

    #[cfg(windows)]
    let file = {
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt;

        // Open the reparse point itself. The metadata check below rejects it
        // rather than allowing Windows to follow a symlink/junction target.
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .with_context(|| {
                format!(
                    "failed to open {label} `{}` without following reparse points",
                    path.display()
                )
            })?
    };

    #[cfg(not(any(unix, windows)))]
    let file = {
        let metadata = std::fs::symlink_metadata(path)
            .with_context(|| format!("failed to inspect {label} `{}`", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(anyhow!(
                "{label} `{}` must not be a symlink",
                path.display()
            ));
        }
        File::open(path).with_context(|| format!("failed to open {label} `{}`", path.display()))?
    };

    let metadata = file
        .metadata()
        .with_context(|| format!("failed to inspect open {label} `{}`", path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "{label} `{}` must be a regular file, not a symlink or special file",
            path.display()
        ));
    }
    let actual = metadata.len();
    if actual > u64::try_from(limit).unwrap_or(u64::MAX) {
        return Err(anyhow!(
            "{label} `{}` exceeds {limit} bytes (actual {actual})",
            path.display()
        ));
    }
    Ok((file, actual))
}

pub fn read_stream_bounded(reader: impl Read, limit: usize, label: &str) -> Result<Vec<u8>> {
    if limit == 0 {
        return Err(anyhow!("{label} byte limit must be positive"));
    }
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    reader
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {label}"))?;
    if bytes.len() > limit {
        return Err(anyhow!("{label} exceeds {limit} bytes"));
    }
    Ok(bytes)
}

pub fn read_file_bounded(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>> {
    let (file, expected_size) = open_regular_file_bounded(path, limit, label)?;
    let bytes = read_stream_bounded(file, limit, &format!("{label} `{}`", path.display()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != expected_size {
        return Err(anyhow!(
            "{label} `{}` changed size while reading (expected {expected_size}, read {})",
            path.display(),
            bytes.len()
        ));
    }
    Ok(bytes)
}

pub fn read_utf8_file_bounded(path: &Path, limit: usize, label: &str) -> Result<String> {
    String::from_utf8(read_file_bounded(path, limit, label)?)
        .with_context(|| format!("{label} `{}` is not UTF-8", path.display()))
}

pub fn read_utf8_stream_bounded(reader: impl Read, limit: usize, label: &str) -> Result<String> {
    String::from_utf8(read_stream_bounded(reader, limit, label)?)
        .with_context(|| format!("{label} is not UTF-8"))
}

struct TemporaryFileCleanup(Option<PathBuf>);

impl Drop for TemporaryFileCleanup {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

/// Atomically publish bounded bytes without following an existing destination
/// symlink. The parent must already be a real directory.
pub fn write_file_atomic_bounded(
    path: &Path,
    bytes: impl AsRef<[u8]>,
    limit: usize,
    label: &str,
) -> Result<()> {
    let bytes = bytes.as_ref();
    if limit == 0 || bytes.len() > limit {
        return Err(anyhow!(
            "{label} output exceeds {limit} bytes (actual {})",
            bytes.len()
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent_metadata = fs::symlink_metadata(parent)
        .with_context(|| format!("failed to inspect {label} parent `{}`", parent.display()))?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.file_type().is_dir() {
        return Err(anyhow!(
            "{label} parent `{}` must be a real directory",
            parent.display()
        ));
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("{label} output path has no filename"))?;

    let mut opened = None;
    for _ in 0..16 {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = std::ffi::OsString::from(".");
        temporary_name.push(file_name);
        temporary_name.push(format!(".axiograph-tmp-{}-{sequence}", std::process::id()));
        let temporary = parent.join(temporary_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => {
                opened = Some((temporary, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to create temporary {label} in `{}`",
                        parent.display()
                    )
                });
            }
        }
    }
    let (temporary, mut file) =
        opened.ok_or_else(|| anyhow!("failed to allocate temporary {label} filename"))?;
    let mut cleanup = TemporaryFileCleanup(Some(temporary.clone()));
    file.write_all(bytes)
        .with_context(|| format!("failed to write temporary {label}"))?;
    file.sync_all()
        .with_context(|| format!("failed to sync temporary {label}"))?;
    drop(file);

    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => {
            return Err(anyhow!(
                "{label} destination `{}` must be absent or a regular file",
                path.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context(format!("failed to inspect {label} destination")),
    }
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to atomically publish {label} `{}`", path.display()))?;
    cleanup.0 = None;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("failed to sync {label} parent `{}`", parent.display()))?;
    Ok(())
}

/// Reject excessive or unbalanced JSON container nesting before Serde builds
/// an allocation graph. Full JSON syntax remains Serde's responsibility.
pub fn validate_json_nesting(bytes: &[u8], max_depth: usize, label: &str) -> Result<()> {
    if max_depth == 0 {
        return Err(anyhow!("{label} JSON nesting limit must be positive"));
    }
    let mut stack = Vec::with_capacity(max_depth.min(32));
    let mut in_string = false;
    let mut escaped = false;
    for &byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                stack.push(byte);
                if stack.len() > max_depth {
                    return Err(anyhow!(
                        "{label} JSON nesting exceeds {max_depth} containers"
                    ));
                }
            }
            b'}' if stack.pop() != Some(b'{') => {
                return Err(anyhow!("{label} JSON containers are unbalanced"));
            }
            b']' if stack.pop() != Some(b'[') => {
                return Err(anyhow!("{label} JSON containers are unbalanced"));
            }
            _ => {}
        }
    }
    if in_string || escaped || !stack.is_empty() {
        return Err(anyhow!("{label} JSON is structurally unbalanced"));
    }
    Ok(())
}

pub fn parse_json_bounded<T: DeserializeOwned>(
    bytes: &[u8],
    limit: usize,
    label: &str,
) -> Result<T> {
    if bytes.len() > limit {
        return Err(anyhow!(
            "{label} exceeds {limit} bytes (actual {})",
            bytes.len()
        ));
    }
    validate_json_nesting(bytes, MAX_JSON_NESTING_DEPTH, label)?;
    serde_json::from_slice(bytes).map_err(|error| anyhow!("invalid {label} JSON: {error}"))
}

pub fn validate_json_value_bounded(
    value: &serde_json::Value,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value).with_context(|| format!("failed to encode {label}"))?;
    if bytes.len() > limit {
        return Err(anyhow!(
            "{label} exceeds {limit} bytes (actual {})",
            bytes.len()
        ));
    }
    validate_json_nesting(&bytes, MAX_JSON_NESTING_DEPTH, label)?;
    Ok(bytes)
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessLimits {
    timeout: Duration,
    max_stdin_bytes: usize,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
}

impl ProcessLimits {
    pub fn new(
        timeout: Duration,
        max_stdin_bytes: usize,
        max_stdout_bytes: usize,
        max_stderr_bytes: usize,
    ) -> Result<Self> {
        if timeout.is_zero() || timeout > MAX_CHILD_RUNTIME {
            return Err(anyhow!(
                "child-process timeout must be in 1ms..={}s",
                MAX_CHILD_RUNTIME.as_secs()
            ));
        }
        for (name, actual, maximum) in [
            ("stdin", max_stdin_bytes, MAX_CHILD_STDIN_BYTES),
            ("stdout", max_stdout_bytes, MAX_CHILD_STDOUT_BYTES),
            ("stderr", max_stderr_bytes, MAX_CHILD_STDERR_BYTES),
        ] {
            if actual == 0 || actual > maximum {
                return Err(anyhow!(
                    "child-process {name} limit must be in 1..={maximum} bytes"
                ));
            }
        }
        Ok(Self {
            timeout,
            max_stdin_bytes,
            max_stdout_bytes,
            max_stderr_bytes,
        })
    }

    pub fn plugin(timeout: Duration) -> Result<Self> {
        Self::new(
            timeout,
            DEFAULT_PLUGIN_STDIN_BYTES,
            DEFAULT_PLUGIN_STDOUT_BYTES,
            MAX_CHILD_STDERR_BYTES,
        )
    }
}

#[derive(Debug)]
struct ChildLimiter {
    active: AtomicUsize,
    maximum: usize,
}

impl ChildLimiter {
    const fn new(maximum: usize) -> Self {
        Self {
            active: AtomicUsize::new(0),
            maximum,
        }
    }

    fn acquire(&self, context: &str) -> Result<ChildPermit<'_>> {
        loop {
            let current = self.active.load(Ordering::Acquire);
            if current >= self.maximum {
                return Err(anyhow!(
                    "{context}: child-process concurrency exceeds {}",
                    self.maximum
                ));
            }
            if self
                .active
                .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(ChildPermit { limiter: self });
            }
        }
    }
}

#[derive(Debug)]
struct ChildPermit<'a> {
    limiter: &'a ChildLimiter,
}

impl Drop for ChildPermit<'_> {
    fn drop(&mut self) {
        self.limiter.active.fetch_sub(1, Ordering::AcqRel);
    }
}

static CHILD_LIMITER: ChildLimiter = ChildLimiter::new(MAX_CONCURRENT_CHILDREN);

enum PipeResult {
    Stdout(Result<Vec<u8>>),
    Stderr(Result<Vec<u8>>),
    Stdin(Result<()>),
}

fn read_pipe_bounded(mut pipe: impl Read, limit: usize, label: &'static str) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let remaining = limit.saturating_add(1).saturating_sub(bytes.len());
        if remaining == 0 {
            return Err(anyhow!("{label} exceeded {limit} bytes"));
        }
        let read_len = buffer.len().min(remaining);
        let count = pipe.read(&mut buffer[..read_len])?;
        if count == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > limit {
            return Err(anyhow!("{label} exceeded {limit} bytes"));
        }
    }
}

fn terminate_child(child: &mut dyn ChildWrapper) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Run one direct command in a new process group (Unix) or job object
/// (Windows), with bounded stdin/stdout/stderr, wall time, and global
/// concurrency. Timeout and output-overflow termination applies to descendants,
/// not only the immediate child.
pub fn run_command_bounded(
    mut command: Command,
    stdin_bytes: &[u8],
    limits: ProcessLimits,
    context: &str,
) -> Result<Output> {
    if stdin_bytes.len() > limits.max_stdin_bytes {
        return Err(anyhow!(
            "{context}: stdin exceeds {} bytes",
            limits.max_stdin_bytes
        ));
    }
    let _permit = CHILD_LIMITER.acquire(context)?;

    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut wrapped = CommandWrap::from(command);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::std::ProcessGroup::leader());
    #[cfg(windows)]
    wrapped.wrap(process_wrap::std::JobObject);

    let mut child = wrapped
        .spawn()
        .with_context(|| format!("{context}: failed to spawn child process group"))?;
    let mut stdin = child
        .stdin()
        .take()
        .ok_or_else(|| anyhow!("{context}: failed to open child stdin"))?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| anyhow!("{context}: failed to open child stdout"))?;
    let stderr = child
        .stderr()
        .take()
        .ok_or_else(|| anyhow!("{context}: failed to open child stderr"))?;

    thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel();
        let stdin_sender = sender.clone();
        scope.spawn(move || {
            let result = stdin
                .write_all(stdin_bytes)
                .context("failed to write child stdin");
            drop(stdin);
            let _ = stdin_sender.send(PipeResult::Stdin(result));
        });
        let stdout_sender = sender.clone();
        scope.spawn(move || {
            let _ = stdout_sender.send(PipeResult::Stdout(read_pipe_bounded(
                stdout,
                limits.max_stdout_bytes,
                "child stdout",
            )));
        });
        scope.spawn(move || {
            let _ = sender.send(PipeResult::Stderr(read_pipe_bounded(
                stderr,
                limits.max_stderr_bytes,
                "child stderr",
            )));
        });

        let started = Instant::now();
        let mut status: Option<ExitStatus> = None;
        let mut stdout_bytes: Option<Vec<u8>> = None;
        let mut stderr_bytes: Option<Vec<u8>> = None;
        let mut stdin_done = false;

        loop {
            while let Ok(message) = receiver.try_recv() {
                match message {
                    PipeResult::Stdout(result) => match result {
                        Ok(bytes) => stdout_bytes = Some(bytes),
                        Err(error) => {
                            terminate_child(child.as_mut());
                            return Err(anyhow!("{context}: {error}"));
                        }
                    },
                    PipeResult::Stderr(result) => match result {
                        Ok(bytes) => stderr_bytes = Some(bytes),
                        Err(error) => {
                            terminate_child(child.as_mut());
                            return Err(anyhow!("{context}: {error}"));
                        }
                    },
                    PipeResult::Stdin(result) => match result {
                        Ok(()) => stdin_done = true,
                        Err(error) => {
                            terminate_child(child.as_mut());
                            return Err(error).with_context(|| context.to_string());
                        }
                    },
                }
            }

            if status.is_none() {
                match child.try_wait() {
                    Ok(next) => status = next,
                    Err(error) => {
                        terminate_child(child.as_mut());
                        return Err(error)
                            .with_context(|| format!("{context}: failed to poll child"));
                    }
                }
            }
            if status.is_some() && stdin_done && stdout_bytes.is_some() && stderr_bytes.is_some() {
                break;
            }
            if started.elapsed() >= limits.timeout {
                terminate_child(child.as_mut());
                return Err(anyhow!(
                    "{context}: timed out after {}ms",
                    limits.timeout.as_millis()
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }

        Ok(Output {
            status: status.expect("bounded child status present"),
            stdout: stdout_bytes.expect("bounded child stdout present"),
            stderr: stderr_bytes.expect("bounded child stderr present"),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_reader_rejects_symlink_special_and_oversize() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let input = directory.path().join("input");
        std::fs::write(&input, b"12345")?;
        assert!(read_file_bounded(&input, 4, "test input")
            .unwrap_err()
            .to_string()
            .contains("exceeds 4 bytes"));

        assert!(read_file_bounded(directory.path(), 16, "test input")
            .unwrap_err()
            .to_string()
            .contains("regular file"));

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&input, directory.path().join("link"))?;
            assert!(
                read_file_bounded(&directory.path().join("link"), 16, "test input")
                    .unwrap_err()
                    .to_string()
                    .contains("without following symlinks")
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn opened_file_handle_is_stable_across_path_replacement() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let input = directory.path().join("input");
        let replacement = directory.path().join("replacement");
        std::fs::write(&input, b"authenticated image")?;
        std::fs::write(&replacement, b"attacker replacement")?;

        let (mut opened, expected_size) = open_regular_file_bounded(&input, 64, "test input")?;
        std::fs::rename(&replacement, &input)?;
        let bytes = read_stream_bounded(&mut opened, 64, "test input")?;

        assert_eq!(expected_size, bytes.len() as u64);
        assert_eq!(bytes, b"authenticated image");
        assert_eq!(std::fs::read(&input)?, b"attacker replacement");
        Ok(())
    }

    #[test]
    fn atomic_writer_rejects_oversize_and_symlink_destination() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let output = directory.path().join("output");
        assert!(write_file_atomic_bounded(&output, b"12345", 4, "test output").is_err());

        #[cfg(unix)]
        {
            let target = directory.path().join("target");
            std::fs::write(&target, b"untouched")?;
            std::os::unix::fs::symlink(&target, &output)?;
            assert!(
                write_file_atomic_bounded(&output, b"replacement", 32, "test output")
                    .unwrap_err()
                    .to_string()
                    .contains("absent or a regular file")
            );
            assert_eq!(std::fs::read(&target)?, b"untouched");
        }
        Ok(())
    }

    #[test]
    fn stream_reader_rejects_unknown_length_overflow() {
        let error = read_stream_bounded(&b"12345"[..], 4, "test stream")
            .expect_err("stream exceeding its cap must reject");
        assert!(error.to_string().contains("exceeds 4 bytes"));
    }

    #[test]
    fn json_depth_and_balance_are_checked_before_deserialization() {
        let nested = format!("{}0{}", "[".repeat(9), "]".repeat(9));
        assert!(validate_json_nesting(nested.as_bytes(), 8, "test").is_err());
        assert!(validate_json_nesting(br#"{"text":"[ignored]"}"#, 8, "test").is_ok());
        assert!(validate_json_nesting(b"{]", 8, "test").is_err());
    }

    #[test]
    fn child_limiter_fails_closed_at_capacity() -> Result<()> {
        let limiter = ChildLimiter::new(2);
        let _first = limiter.acquire("test")?;
        let second = limiter.acquire("test")?;
        assert!(limiter
            .acquire("test")
            .unwrap_err()
            .to_string()
            .contains("concurrency exceeds 2"));
        drop(second);
        let _replacement = limiter.acquire("test")?;
        Ok(())
    }

    #[cfg(unix)]
    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    #[cfg(unix)]
    #[test]
    fn bounded_child_rejects_output_flood_timeout_and_kills_descendants() -> Result<()> {
        let limits = ProcessLimits::new(Duration::from_secs(2), 16, 32, 32)?;
        let flood = run_command_bounded(shell("yes x"), b"", limits, "flood child").unwrap_err();
        assert!(flood.to_string().contains("stdout exceeded 32 bytes"));

        let directory = tempfile::tempdir()?;
        let marker = directory.path().join("descendant-escaped");
        let mut command = shell("(sleep 0.3; printf escaped > \"$MARKER\") & wait");
        command.env("MARKER", &marker);
        let timeout = run_command_bounded(
            command,
            b"",
            ProcessLimits::new(Duration::from_millis(50), 16, 32, 32)?,
            "sleep child",
        )
        .unwrap_err();
        assert!(timeout.to_string().contains("timed out"));
        thread::sleep(Duration::from_millis(500));
        assert!(
            !marker.exists(),
            "timed-out descendant escaped its process group"
        );
        Ok(())
    }
}
