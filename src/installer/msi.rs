//! MSI extraction backend abstraction (spec 05 §2-4).
//!
//! Windows keeps the `msiexec /a` administrative-extraction backend behind an
//! injectable process runner so the wrapper semantics (argument array, retry
//! on 1618, exit-code diagnostics, verbose logging, no `/i`, no shell) are
//! unit-testable. A native Rust MSI-database backend is NOT shipped: it stays
//! behind [`MsiBackendPreference::Native`]/`Fallback` and reports itself
//! unsupported until the parity gate documented in
//! `agent-work/PHASES/P04-msi-feasibility.md` is met with real fixtures.

use std::path::Path;
use std::time::Duration;

use crate::error::{MsvcKitError, Result};

/// Backend selection for MSI extraction (spec 05 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MsiBackendPreference {
    /// Use the verified-stable backend (currently `msiexec /a`).
    #[default]
    Auto,
    /// Force the Windows Installer administrative extraction backend.
    Msiexec,
    /// Force the native Rust MSI-database backend. Unsupported today: every
    /// call reports an error until the parity gate passes.
    Native,
    /// Primary backend with fallback to the next one on recoverable errors.
    /// Unsupported today (same parity gate as `Native`).
    Fallback,
}

impl MsiBackendPreference {
    /// Parse a backend preference from a CLI/library string.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "msiexec" => Ok(Self::Msiexec),
            "native" => Ok(Self::Native),
            "fallback" => Ok(Self::Fallback),
            other => Err(MsvcKitError::Config(format!(
                "Unknown MSI backend preference {other:?}; valid: auto, msiexec, native, fallback"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Msiexec => "msiexec",
            Self::Native => "native",
            Self::Fallback => "fallback",
        }
    }
}

/// Resolve a preference to the backend identifier that would run. The native
/// and fallback arms report the parity gate instead of pretending to work.
pub fn resolve_backend_id(preference: MsiBackendPreference) -> Result<&'static str> {
    match preference {
        MsiBackendPreference::Auto | MsiBackendPreference::Msiexec => Ok("msiexec-a"),
        MsiBackendPreference::Native | MsiBackendPreference::Fallback => {
            Err(MsvcKitError::UnsupportedPlatform(
                "Native MSI backends are not enabled: the parity gate (real MSI/CAB fixtures \
                 compared against msiexec /a output) has not been met; use msiexec or auto"
                    .to_string(),
            ))
        }
    }
}

/// One process invocation observed by the runner (test seam record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub program: String,
    pub args: Vec<String>,
}

/// The outcome of one process invocation.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub exit_code: Option<i32>,
}

/// Injectable process runner (spec 08 §8: fake process runner tests).
pub trait MsiProcessRunner: Send + Sync {
    /// Run a program with an argument array (never a shell line).
    fn run(&self, program: &str, args: &[String]) -> std::io::Result<RunOutcome>;

    /// Sleep between 1618 retries (injectable so tests do not really wait).
    fn sleep(&self, delay: Duration);

    /// How many 1618 retries are allowed before giving up.
    fn max_retries(&self) -> u32 {
        5
    }
}

/// Synchronous SHA256 file hash (called from the blocking msiexec path).
pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// First actionable failure line from an msiexec verbose log (`/L*V`).
///
/// msiexec writes these logs as UTF-16LE with a BOM on Windows. The exit
/// code alone is often generic (1603), while the log names the real cause
/// — e.g. `Error 1304 ... Error writing to file: <path>` when the target
/// path exceeds MAX_PATH 260. Return the first such line (trimmed) so the
/// caller can surface it verbatim.
fn first_installer_failure_line(log_path: &Path) -> Option<String> {
    let bytes = std::fs::read(log_path).ok()?;
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    text.lines()
        .find(|line| {
            (line.contains("Error 1304")
                || line.contains("Error 1606")
                || line.contains("Could not access network location"))
                && !line.contains("Note: 1: 1304 2:")
        })
        .or_else(|| {
            // Fall back to the raw Note line naming the offending path.
            text.lines()
                .find(|line| line.contains("Note: 1: 1304 2:") || line.contains("Note: 1: 1606 2:"))
        })
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

/// The production runner: real processes, fixed 2-second retry delay.
#[derive(Debug, Default)]
pub struct SystemRunner;

impl MsiProcessRunner for SystemRunner {
    fn run(&self, program: &str, args: &[String]) -> std::io::Result<RunOutcome> {
        let status = std::process::Command::new(program).args(args).status()?;
        Ok(RunOutcome {
            exit_code: status.code(),
        })
    }

    fn sleep(&self, delay: Duration) {
        std::thread::sleep(delay);
    }
}

/// The `msiexec /a` administrative extraction backend (Windows).
#[cfg(windows)]
pub struct MsiexecBackend<'a> {
    runner: &'a dyn MsiProcessRunner,
    /// Fixed retry delay (2 seconds in production).
    retry_delay: Duration,
}

#[cfg(windows)]
impl std::fmt::Debug for MsiexecBackend<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MsiexecBackend").finish_non_exhaustive()
    }
}

#[cfg(windows)]
impl<'a> MsiexecBackend<'a> {
    pub fn new(runner: &'a dyn MsiProcessRunner) -> Self {
        Self {
            runner,
            retry_delay: Duration::from_millis(2000),
        }
    }

    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    /// The exact argv msiexec is invoked with. `/a` is administrative
    /// extraction; `/qn` only suppresses UI; `/L*V` writes a verbose log
    /// beside the source MSI for diagnostics. There is no `/i` anywhere.
    fn build_args(msi_path: &str, target_dir: &str, log_path: &str) -> Vec<String> {
        vec![
            "/a".to_string(),
            msi_path.to_string(),
            "/qn".to_string(),
            format!("TARGETDIR={target_dir}"),
            "/L*V".to_string(),
            log_path.to_string(),
        ]
    }

    /// Extract `msi_path` into `target_dir` via `msiexec /a`.
    ///
    /// Error 1618 (another installation in progress) is retried with a fixed
    /// delay up to the runner's budget; every other non-zero exit is final and
    /// carries the log path plus exit-code-specific guidance. No elevation is
    /// ever requested; MSI-internal custom actions may still require it, in
    /// which case the failure is reported (spec 05 §4).
    ///
    /// G-4 (R3): when `expected_sha256` is provided, the MSI is re-hashed
    /// before extraction and a mismatch aborts. This enforces the trust
    /// boundary: only verified official payloads are passed to msiexec.
    pub fn extract(
        &self,
        msi_path: &Path,
        target_dir: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<()> {
        if let Some(expected) = expected_sha256 {
            let actual = sha256_file(msi_path)?;
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(MsvcKitError::HashMismatch {
                    file: msi_path.display().to_string(),
                    expected: expected.to_string(),
                    actual,
                });
            }
        }
        let msi_arg = super::extractor::msiexec_path_arg(msi_path);
        // build_args prepends `TARGETDIR=`; pass the bare path here. Passing
        // an already-prefixed value produced `TARGETDIR=TARGETDIR=<path>`,
        // which Windows Installer reports as Error 1606 ("could not access
        // network location") and then surfaces as exit code 1603 for every
        // SDK MSI — see the msiexec verbose logs.
        let target_arg = super::extractor::msiexec_path_arg(target_dir);
        let file_name = msi_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown.msi")
            .to_string();
        let log_path = msi_path.with_file_name(format!("msvc-kit-{file_name}.log"));
        let log_arg = super::extractor::msiexec_path_arg(&log_path);
        let args = Self::build_args(&msi_arg, &target_arg, &log_arg);

        let max_retries = self.runner.max_retries();
        let mut attempts = 0u32;
        loop {
            attempts += 1;
            let outcome = self.runner.run("msiexec", &args).map_err(|error| {
                MsvcKitError::Other(format!(
                    "Failed to run msiexec for {file_name}: {error} (is the Windows \
                         Installer service available?)"
                ))
            })?;
            match outcome.exit_code {
                Some(0) => return Ok(()),
                Some(1618) if attempts <= max_retries => {
                    tracing::warn!(
                        "msiexec returned 1618 (another installation in progress) for \
                         {file_name}, retry {attempts}/{max_retries}"
                    );
                    self.runner.sleep(self.retry_delay);
                }
                code => {
                    let inner = first_installer_failure_line(&log_path);
                    let inner_note = match &inner {
                        Some(line) if line.contains("1304") => format!(
                            "\nInner error: {line}\nHint: Windows Installer is limited to \
                             MAX_PATH (260) characters; this usually means the target \
                             directory is too deep for the SDK's Reference/WinMD paths. Use a \
                             shorter installation directory."
                        ),
                        Some(line) => format!("\nInner error: {line}"),
                        None => String::new(),
                    };
                    return Err(MsvcKitError::Other(format!(
                        "msiexec failed with exit code {:?} for {}\n{}\nSource MSI: {}\nTarget \
                         directory: {}\nVerbose log: {}{}",
                        code,
                        file_name,
                        super::extractor::explain_windows_installer_code(code),
                        msi_path.display(),
                        target_dir.display(),
                        log_path.display(),
                        inner_note
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRunner {
        script: Mutex<Vec<Option<i32>>>,
        runs: Mutex<Vec<RunRecord>>,
        sleeps: Mutex<Vec<u64>>,
    }

    impl FakeRunner {
        fn with_codes(codes: &[Option<i32>]) -> Self {
            Self {
                script: Mutex::new(codes.to_vec()),
                ..Default::default()
            }
        }

        fn recorded_runs(&self) -> Vec<RunRecord> {
            self.runs.lock().unwrap().clone()
        }
    }

    impl MsiProcessRunner for FakeRunner {
        fn run(&self, program: &str, args: &[String]) -> std::io::Result<RunOutcome> {
            self.runs.lock().unwrap().push(RunRecord {
                program: program.to_string(),
                args: args.to_vec(),
            });
            let mut script = self.script.lock().unwrap();
            let code = if script.is_empty() {
                Some(0)
            } else {
                script.remove(0)
            };
            Ok(RunOutcome { exit_code: code })
        }

        fn sleep(&self, delay: Duration) {
            self.sleeps.lock().unwrap().push(delay.as_millis() as u64);
        }

        fn max_retries(&self) -> u32 {
            3
        }
    }

    fn msi_fixture(dir: &Path) -> PathBuf {
        let path = dir.join("fixture.msi");
        std::fs::write(&path, b"not a real msi - the fake runner never reads it").unwrap();
        path
    }

    #[test]
    fn installer_failure_line_is_extracted_from_utf16_verbose_log() {
        // msiexec writes /L*V logs as UTF-16LE with a BOM. The interesting
        // line names the real cause behind a generic 1603: Error 1304 with
        // the offending path (MAX_PATH overflow).
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("msvc-kit-fixture.msi.log");
        let body = concat!(
            "=== Verbose logging started ===\r\n",
            "MSI (s) (60:6C) [17:14:19:813]: Note: 1: 1304 2: D:\\very\\deep\\path.winmd\r\n",
            "MSI (s) (60:6C) [17:14:19:813]: Product: Foo -- Error 1304. Error writing to file: ",
            "D:\\very\\deep\\path.winmd.  Verify that you have access to that directory.\r\n",
            "MSI (c) (14:4C) [17:14:19:905]: MainEngineThread is returning 1603\r\n",
        );
        let mut bytes = vec![0xFF, 0xFE];
        for unit in body.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(&log, bytes).unwrap();

        let line = first_installer_failure_line(&log).expect("failure line");
        assert!(line.contains("Error 1304"), "line: {line}");
        assert!(line.contains("path.winmd"), "line: {line}");
    }

    #[test]
    fn installer_failure_line_is_none_for_clean_log() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("clean.log");
        std::fs::write(&log, b"=== Verbose logging started ===\r\nall good\r\n").unwrap();
        assert!(first_installer_failure_line(&log).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn success_runs_administrative_extraction_with_verbose_log_and_no_install() {
        let runner = FakeRunner::with_codes(&[Some(0)]);
        let backend = MsiexecBackend::new(&runner);
        let dir = tempfile::tempdir().unwrap();
        let msi = msi_fixture(dir.path());
        let target = dir.path().join("target");

        backend.extract(&msi, &target, None).unwrap();

        let runs = runner.recorded_runs();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].program, "msiexec");
        let args = &runs[0].args;
        assert_eq!(args[0], "/a", "must be administrative extraction");
        assert_eq!(args[2], "/qn");
        // Exactly one `TARGETDIR=` prefix: a double prefix
        // (`TARGETDIR=TARGETDIR=<path>`) makes Windows Installer fail every
        // MSI with Error 1606 -> exit code 1603, so assert the exact form.
        assert_eq!(
            args[3],
            format!("TARGETDIR={}", target.display()),
            "TARGETDIR must carry exactly one prefix and the bare path"
        );
        assert!(
            !args[3].contains("TARGETDIR=TARGETDIR="),
            "double TARGETDIR prefix must never be produced"
        );
        assert!(
            args.windows(2).any(|pair| pair[0] == "/L*V"),
            "verbose log flag required"
        );
        assert!(
            !args.iter().any(|arg| arg == "/i"),
            "product install /i is forbidden"
        );
        assert!(
            !args.iter().any(|arg| arg.contains("runas")),
            "no elevation requests"
        );
        // The verbose log sits beside the source MSI.
        let log_arg = &args[args.len() - 1];
        assert_eq!(
            Path::new(log_arg).parent(),
            msi.parent(),
            "log beside the source MSI"
        );
    }

    #[cfg(windows)]
    #[test]
    fn matching_digest_runs_msiexec() {
        // G-4 (R3): the MSI is re-hashed before the installer runs; a digest
        // that matches the file on disk lets the extraction proceed.
        let runner = FakeRunner::with_codes(&[Some(0)]);
        let backend = MsiexecBackend::new(&runner);
        let dir = tempfile::tempdir().unwrap();
        let msi = msi_fixture(dir.path());
        let digest = sha256_file(&msi).unwrap();

        backend
            .extract(&msi, &dir.path().join("target"), Some(&digest))
            .unwrap();

        assert_eq!(runner.recorded_runs().len(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn mismatching_digest_aborts_before_msiexec_runs() {
        // G-4 (R3): a payload that no longer matches its official digest must
        // never reach the Windows Installer. The check happens before any
        // process is spawned, so zero runs are recorded.
        let runner = FakeRunner::with_codes(&[Some(0)]);
        let backend = MsiexecBackend::new(&runner);
        let dir = tempfile::tempdir().unwrap();
        let msi = msi_fixture(dir.path());
        let wrong = "0".repeat(64);

        let error = backend
            .extract(&msi, &dir.path().join("target"), Some(&wrong))
            .unwrap_err();

        assert!(
            matches!(error, MsvcKitError::HashMismatch { .. }),
            "expected a hash mismatch, got: {error}"
        );
        assert!(
            runner.recorded_runs().is_empty(),
            "msiexec must not run for a payload that fails the digest check"
        );
    }

    #[cfg(windows)]
    #[test]
    fn transient_1618_is_retried_then_succeeds() {
        let runner = FakeRunner::with_codes(&[Some(1618), Some(1618), Some(0)]);
        let backend = MsiexecBackend::new(&runner).with_retry_delay(Duration::from_millis(1));
        let dir = tempfile::tempdir().unwrap();
        backend
            .extract(&msi_fixture(dir.path()), &dir.path().join("t"), None)
            .unwrap();

        assert_eq!(runner.recorded_runs().len(), 3);
        let sleeps = runner.sleeps.lock().unwrap().clone();
        assert_eq!(sleeps, vec![1, 1], "one delay per retry, configurable");
    }

    #[cfg(windows)]
    #[test]
    fn exhausted_1618_retries_fail_with_context() {
        let runner = FakeRunner::with_codes(&[Some(1618); 5]);
        let backend = MsiexecBackend::new(&runner).with_retry_delay(Duration::from_millis(1));
        let dir = tempfile::tempdir().unwrap();
        let error = backend
            .extract(&msi_fixture(dir.path()), &dir.path().join("t"), None)
            .unwrap_err();
        assert!(error.to_string().contains("1618"), "{error}");
        // Initial attempt + 3 retries (max_retries budget from the runner).
        assert_eq!(runner.recorded_runs().len(), 4);
    }

    #[cfg(windows)]
    #[test]
    fn fatal_codes_report_log_path_and_guidance() {
        for (code, expected) in [
            (1603, "generic fatal"),
            (1619, "could not be opened"),
            (1620, "not a valid"),
        ] {
            let runner = FakeRunner::with_codes(&[Some(code)]);
            let backend = MsiexecBackend::new(&runner);
            let dir = tempfile::tempdir().unwrap();
            let msi = msi_fixture(dir.path());
            let error = backend
                .extract(&msi, &dir.path().join("t"), None)
                .unwrap_err()
                .to_string();
            assert!(error.contains(&code.to_string()), "{error}");
            assert!(error.contains(expected), "{code}: {error}");
            assert!(
                error.contains("Verbose log:"),
                "diagnostics must point at the verbose log: {error}"
            );
            assert!(
                error.contains("msvc-kit-fixture.msi.log"),
                "log path beside the source MSI: {error}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn spawn_failure_is_reported_without_retry() {
        struct FailingRunner;
        impl MsiProcessRunner for FailingRunner {
            fn run(&self, _program: &str, _args: &[String]) -> std::io::Result<RunOutcome> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "msiexec missing",
                ))
            }
            fn sleep(&self, _delay: Duration) {}
        }
        let backend = MsiexecBackend::new(&FailingRunner);
        let dir = tempfile::tempdir().unwrap();
        let error = backend
            .extract(&msi_fixture(dir.path()), &dir.path().join("t"), None)
            .unwrap_err();
        assert!(error.to_string().contains("msiexec missing"), "{error}");
    }

    #[test]
    fn backend_preferences_parse_and_resolve() {
        for value in ["auto", "msiexec", "native", "fallback"] {
            assert!(MsiBackendPreference::parse(value).is_ok(), "{value}");
        }
        assert_eq!(
            MsiBackendPreference::parse("MSIEXEC").unwrap(),
            MsiBackendPreference::Msiexec
        );
        assert!(MsiBackendPreference::parse("bogus").is_err());

        assert_eq!(
            resolve_backend_id(MsiBackendPreference::Auto).unwrap(),
            "msiexec-a"
        );
        assert_eq!(
            resolve_backend_id(MsiBackendPreference::Msiexec).unwrap(),
            "msiexec-a"
        );
        for unsupported in [MsiBackendPreference::Native, MsiBackendPreference::Fallback] {
            let error = resolve_backend_id(unsupported).unwrap_err().to_string();
            assert!(error.contains("parity gate"), "{error}");
        }
    }
}
