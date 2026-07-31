use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::{log_store::LogSnapshot, DiagnosticReport, DiagnosticsError, Redactor};

const REPORT_ARCHIVE_NAME: &str = "diagnostics.json";

pub struct ExportOutcome {
    path: PathBuf,
    bytes: u64,
    entry_count: usize,
}

impl ExportOutcome {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    pub const fn entry_count(&self) -> usize {
        self.entry_count
    }
}

impl fmt::Debug for ExportOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExportOutcome")
            .field("path", &"[REDACTED_PATH]")
            .field("bytes", &self.bytes)
            .field("entry_count", &self.entry_count)
            .finish()
    }
}

pub(crate) fn export_bundle(
    destination: &Path,
    report: &DiagnosticReport,
    logs: Vec<LogSnapshot>,
    redactor: &Redactor,
    max_export_bytes: u64,
) -> Result<ExportOutcome, DiagnosticsError> {
    reject_existing_destination(destination)?;

    let serialized_report =
        serde_json::to_string_pretty(report).map_err(DiagnosticsError::SerializeReport)?;
    let serialized_report = redactor.redact(&serialized_report);
    redactor.audit(&serialized_report)?;
    let report_bytes = serialized_report.into_bytes();
    let content_bytes = logs.iter().try_fold(
        u64::try_from(report_bytes.len()).map_err(|_| DiagnosticsError::ExportSizeLimitExceeded)?,
        |total, log| {
            let log_bytes = u64::try_from(log.content.len())
                .map_err(|_| DiagnosticsError::ExportSizeLimitExceeded)?;
            total
                .checked_add(log_bytes)
                .ok_or(DiagnosticsError::ExportSizeLimitExceeded)
        },
    )?;
    if content_bytes > max_export_bytes {
        return Err(DiagnosticsError::ExportSizeLimitExceeded);
    }

    let mut pending = PendingExport::create(destination)?;
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o600);
    let mut archive = ZipWriter::new(pending.file()?);
    archive
        .start_file(REPORT_ARCHIVE_NAME, options)
        .map_err(DiagnosticsError::WriteArchive)?;
    archive
        .write_all(&report_bytes)
        .map_err(|source| DiagnosticsError::io("write report to archive", source))?;

    for log in &logs {
        validate_archive_log_name(&log.archive_name)?;
        archive
            .start_file(&log.archive_name, options)
            .map_err(DiagnosticsError::WriteArchive)?;
        archive
            .write_all(&log.content)
            .map_err(|source| DiagnosticsError::io("write log to archive", source))?;
    }

    let file = archive.finish().map_err(DiagnosticsError::WriteArchive)?;
    file.sync_all()
        .map_err(|source| DiagnosticsError::io("sync diagnostics archive", source))?;
    let archive_bytes = file
        .metadata()
        .map_err(|source| DiagnosticsError::io("inspect diagnostics archive", source))?
        .len();
    if archive_bytes > max_export_bytes {
        return Err(DiagnosticsError::ExportSizeLimitExceeded);
    }
    drop(file);

    let path = pending.publish(destination)?;

    Ok(ExportOutcome {
        path,
        bytes: archive_bytes,
        entry_count: logs.len().saturating_add(1),
    })
}

fn validate_archive_log_name(name: &str) -> Result<(), DiagnosticsError> {
    let Some(file_name) = name.strip_prefix("logs/") else {
        return Err(DiagnosticsError::UnsafeArchiveEntry);
    };
    if file_name == "app.log" {
        return Ok(());
    }

    let valid_rotation = file_name
        .strip_prefix("app.")
        .and_then(|value| value.strip_suffix(".log"))
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|index| index > 0);
    if valid_rotation {
        Ok(())
    } else {
        Err(DiagnosticsError::UnsafeArchiveEntry)
    }
}

fn reject_existing_destination(destination: &Path) -> Result<(), DiagnosticsError> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(DiagnosticsError::DestinationExists),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(DiagnosticsError::io(
            "inspect diagnostics destination",
            source,
        )),
    }
}

struct PendingExport {
    path: PathBuf,
    file: Option<File>,
    published: bool,
}

impl PendingExport {
    fn create(destination: &Path) -> Result<Self, DiagnosticsError> {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let metadata = fs::symlink_metadata(parent)
            .map_err(|source| DiagnosticsError::io("inspect export directory", source))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(DiagnosticsError::UnsafeExportDirectory);
        }

        let path = parent.join(format!(
            ".dada-assistant-diagnostics-{}.part",
            Uuid::new_v4()
        ));
        let file = create_private_new_file(&path)?;
        Ok(Self {
            path,
            file: Some(file),
            published: false,
        })
    }

    fn file(&mut self) -> Result<File, DiagnosticsError> {
        self.file
            .take()
            .ok_or(DiagnosticsError::ExportAlreadyFinalized)
    }

    fn publish(mut self, destination: &Path) -> Result<PathBuf, DiagnosticsError> {
        reject_existing_destination(destination)?;

        // The completed temporary file and destination are in the same
        // directory. A hard link publishes the complete inode atomically and
        // fails rather than overwriting an existing destination.
        fs::hard_link(&self.path, destination)
            .map_err(|source| DiagnosticsError::io("publish diagnostics archive", source))?;
        set_private_export_permissions(destination)?;

        if let Err(source) = fs::remove_file(&self.path) {
            let _ = fs::remove_file(destination);
            return Err(DiagnosticsError::io(
                "remove diagnostics temporary file",
                source,
            ));
        }
        self.published = true;
        Ok(destination.to_path_buf())
    }
}

impl Drop for PendingExport {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn create_private_new_file(path: &Path) -> Result<File, DiagnosticsError> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|source| DiagnosticsError::io("create diagnostics archive", source))
}

#[cfg(not(unix))]
fn create_private_new_file(path: &Path) -> Result<File, DiagnosticsError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| DiagnosticsError::io("create diagnostics archive", source))
}

#[cfg(unix)]
fn set_private_export_permissions(path: &Path) -> Result<(), DiagnosticsError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|source| DiagnosticsError::io("secure diagnostics archive", source))
}

#[cfg(not(unix))]
fn set_private_export_permissions(_path: &Path) -> Result<(), DiagnosticsError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagnosticReportInput, OperatingSystem};

    #[test]
    fn outcome_debug_does_not_reveal_destination() {
        let outcome = ExportOutcome {
            path: PathBuf::from("/Users/alice/Desktop/private.zip"),
            bytes: 42,
            entry_count: 2,
        };

        let debug = format!("{outcome:?}");
        assert!(!debug.contains("alice"));
        assert!(debug.contains("[REDACTED_PATH]"));
    }

    #[test]
    fn validates_only_fixed_log_entry_names() {
        for valid in ["logs/app.log", "logs/app.1.log", "logs/app.42.log"] {
            validate_archive_log_name(valid).expect("name should be valid");
        }
        for invalid in [
            "recovery.json",
            "logs/recovery.json",
            "logs/../recovery.json",
            "/logs/app.log",
            "logs/app.0.log",
        ] {
            assert!(validate_archive_log_name(invalid).is_err());
        }
    }

    #[test]
    fn pending_export_is_removed_when_not_published() {
        let temp = tempfile::TempDir::new().expect("temporary directory should be created");
        let destination = temp.path().join("diagnostics.zip");
        let pending = PendingExport::create(&destination).expect("pending file should be created");
        let pending_path = pending.path.clone();
        assert!(pending_path.exists());

        drop(pending);

        assert!(!pending_path.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn archive_size_failure_removes_temporary_file() {
        let temp = tempfile::TempDir::new().expect("temporary directory should be created");
        let destination = temp.path().join("diagnostics.zip");
        let redactor = Redactor::new().expect("redactor should initialize");
        let report = DiagnosticReport::create(
            "wocao-hub",
            DiagnosticReportInput {
                application_version: "1.0.0".to_owned(),
                operating_system: OperatingSystem::Macos,
                architecture: crate::Architecture::Aarch64,
                build_profile: crate::BuildProfile::Release,
                checks: crate::DiagnosticChecks::default(),
            },
            0,
            0,
        )
        .expect("report should be created");
        let report_content =
            serde_json::to_string_pretty(&report).expect("report should serialize");
        let content_limit = u64::try_from(redactor.redact(&report_content).len())
            .expect("report should fit in u64");

        let error = export_bundle(&destination, &report, Vec::new(), &redactor, content_limit)
            .expect_err("ZIP framing should exceed the content-only limit");

        assert!(matches!(error, DiagnosticsError::ExportSizeLimitExceeded));
        assert!(!destination.exists());
        let remaining_names: Vec<_> = fs::read_dir(temp.path())
            .expect("temporary directory should be readable")
            .map(|entry| {
                entry
                    .expect("directory entry should be readable")
                    .file_name()
            })
            .collect();
        assert!(remaining_names.is_empty());
    }
}
