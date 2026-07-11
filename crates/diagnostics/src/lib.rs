mod export;
mod log_store;
mod redaction;
mod report;

use std::{fmt, io, path::Path};

use chrono::{SecondsFormat, Utc};
pub use export::ExportOutcome;
pub use log_store::DiagnosticsWriterFactory;
pub use redaction::Redactor;
pub use report::{
    Architecture, BuildProfile, CheckState, DiagnosticChecks, DiagnosticReport,
    DiagnosticReportInput, OperatingSystem, DIAGNOSTIC_REPORT_SCHEMA_VERSION,
};
use serde::Serialize;
use thiserror::Error;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const MAX_CONFIGURED_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CONFIGURED_FILES: usize = 32;
const MAX_CONFIGURED_RECORD_BYTES: usize = 1024 * 1024;
const REPORT_EXPORT_BUDGET_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticsConfig {
    pub max_file_bytes: u64,
    pub max_files: usize,
    pub max_record_bytes: usize,
    pub max_export_bytes: u64,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
        const MAX_FILES: usize = 5;
        Self {
            max_file_bytes: MAX_FILE_BYTES,
            max_files: MAX_FILES,
            max_record_bytes: 64 * 1024,
            max_export_bytes: MAX_FILE_BYTES * MAX_FILES as u64 + REPORT_EXPORT_BUDGET_BYTES,
        }
    }
}

impl DiagnosticsConfig {
    fn validate(self) -> Result<Self, DiagnosticsError> {
        if self.max_file_bytes == 0 || self.max_file_bytes > MAX_CONFIGURED_FILE_BYTES {
            return Err(DiagnosticsError::InvalidConfiguration("max_file_bytes"));
        }
        if self.max_files == 0 || self.max_files > MAX_CONFIGURED_FILES {
            return Err(DiagnosticsError::InvalidConfiguration("max_files"));
        }
        if self.max_record_bytes == 0
            || self.max_record_bytes > MAX_CONFIGURED_RECORD_BYTES
            || u64::try_from(self.max_record_bytes).unwrap_or(u64::MAX) > self.max_file_bytes
        {
            return Err(DiagnosticsError::InvalidConfiguration("max_record_bytes"));
        }

        let retained_log_budget = self
            .max_file_bytes
            .checked_mul(
                u64::try_from(self.max_files)
                    .map_err(|_| DiagnosticsError::InvalidConfiguration("max_files"))?,
            )
            .and_then(|value| value.checked_add(REPORT_EXPORT_BUDGET_BYTES))
            .ok_or(DiagnosticsError::InvalidConfiguration("max_export_bytes"))?;
        if self.max_export_bytes < retained_log_budget {
            return Err(DiagnosticsError::InvalidConfiguration("max_export_bytes"));
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct Diagnostics {
    service_name: String,
    config: DiagnosticsConfig,
    redactor: Redactor,
    logs: log_store::LogStore,
}

impl Diagnostics {
    pub fn open(
        data_directory: impl AsRef<Path>,
        service_name: impl AsRef<str>,
        config: DiagnosticsConfig,
    ) -> Result<Self, DiagnosticsError> {
        let config = config.validate()?;
        let service_name = service_name.as_ref();
        report::validate_service_name(service_name)?;
        let redactor = Redactor::new()?;
        let logs = log_store::LogStore::open(data_directory.as_ref(), config, redactor.clone())?;

        Ok(Self {
            service_name: service_name.to_owned(),
            config,
            redactor,
            logs,
        })
    }

    pub fn record(
        &self,
        level: LogLevel,
        target: &str,
        message: &str,
    ) -> Result<(), DiagnosticsError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct LogRecord<'record> {
            timestamp: String,
            level: LogLevel,
            target: &'record str,
            message: &'record str,
        }

        let target = self.redactor.redact(target);
        let message = self.redactor.redact(message);
        let record = LogRecord {
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            level,
            target: &target,
            message: &message,
        };
        let serialized = serde_json::to_string(&record).map_err(DiagnosticsError::SerializeLog)?;
        self.logs.append(&serialized)
    }

    pub fn tracing_writer(&self) -> DiagnosticsWriterFactory {
        self.logs.writer_factory()
    }

    pub fn create_report(
        &self,
        input: DiagnosticReportInput,
    ) -> Result<DiagnosticReport, DiagnosticsError> {
        let stats = self.logs.stats()?;
        DiagnosticReport::create(
            &self.service_name,
            input,
            stats.file_count,
            stats.total_bytes,
        )
    }

    pub fn export_bundle(
        &self,
        destination: impl AsRef<Path>,
        report: &DiagnosticReport,
    ) -> Result<ExportOutcome, DiagnosticsError> {
        if report.service_name() != self.service_name {
            return Err(DiagnosticsError::ReportServiceMismatch);
        }
        let logs = self.logs.snapshots_for_export()?;
        export::export_bundle(
            destination.as_ref(),
            report,
            logs,
            &self.redactor,
            self.config.max_export_bytes,
        )
    }

    pub fn prune_logs(&self) -> Result<(), DiagnosticsError> {
        self.logs.prune()
    }
}

impl fmt::Debug for Diagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Diagnostics")
            .field("service_name", &self.service_name)
            .field("config", &self.config)
            .field("data_directory", &"[REDACTED_PATH]")
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum DiagnosticsError {
    #[error("invalid diagnostics configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error("invalid diagnostics metadata: {0}")]
    InvalidMetadata(&'static str),
    #[error("failed to initialize diagnostics redaction")]
    RedactorInitializationFailed,
    #[error("diagnostics I/O failed during {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("diagnostics log lock is unavailable")]
    LogLockPoisoned,
    #[error("diagnostics log path is unsafe")]
    UnsafeLogPath,
    #[error("diagnostics export directory is unsafe")]
    UnsafeExportDirectory,
    #[error("diagnostics archive entry is unsafe")]
    UnsafeArchiveEntry,
    #[error("diagnostics destination already exists")]
    DestinationExists,
    #[error("diagnostics log exceeds its configured size limit")]
    LogSizeLimitExceeded,
    #[error("diagnostics export exceeds its configured size limit")]
    ExportSizeLimitExceeded,
    #[error("sensitive data remained after diagnostics redaction")]
    SensitiveDataDetected,
    #[error("diagnostics report belongs to a different service")]
    ReportServiceMismatch,
    #[error("diagnostics export was already finalized")]
    ExportAlreadyFinalized,
    #[error("failed to serialize diagnostics log record")]
    SerializeLog(#[source] serde_json::Error),
    #[error("failed to serialize diagnostics report")]
    SerializeReport(#[source] serde_json::Error),
    #[error("failed to write diagnostics archive")]
    WriteArchive(#[source] zip::result::ZipError),
    #[error("diagnostics subscriber is already initialized")]
    SubscriberInitializationFailed,
}

impl DiagnosticsError {
    pub(crate) fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

/// Initializes console tracing for compatibility with the existing services.
/// Persistent private logs require [`init_with_diagnostics`].
pub fn init(service_name: &'static str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .try_init();
    tracing::info!(service = service_name, "diagnostics initialized");
}

pub fn init_with_diagnostics(
    service_name: &'static str,
    diagnostics: &Diagnostics,
) -> Result<(), DiagnosticsError> {
    if diagnostics.service_name != service_name {
        return Err(DiagnosticsError::ReportServiceMismatch);
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_writer(diagnostics.tracing_writer()),
        )
        .try_init()
        .map_err(|_| DiagnosticsError::SubscriberInitializationFailed)?;
    tracing::info!(service = service_name, "diagnostics initialized");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Read};

    use tempfile::TempDir;
    use tracing_subscriber::layer::SubscriberExt;
    use zip::ZipArchive;

    use super::*;

    fn test_config() -> DiagnosticsConfig {
        DiagnosticsConfig {
            max_file_bytes: 4 * 1024,
            max_files: 3,
            max_record_bytes: 1024,
            max_export_bytes: 3 * 4 * 1024 + REPORT_EXPORT_BUDGET_BYTES,
        }
    }

    fn report_input() -> DiagnosticReportInput {
        DiagnosticReportInput {
            application_version: "1.2.3".to_owned(),
            operating_system: OperatingSystem::Macos,
            architecture: Architecture::Aarch64,
            build_profile: BuildProfile::Release,
            checks: DiagnosticChecks {
                desktop_app: CheckState::Healthy,
                locale_configuration: CheckState::Healthy,
                route_bundle: CheckState::Degraded,
                local_proxy: CheckState::NotChecked,
                network_recovery: CheckState::Healthy,
                official_downloads: CheckState::Unavailable,
            },
        }
    }

    #[test]
    fn records_structured_redacted_log_entries() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let diagnostics = Diagnostics::open(temp.path(), "wocao-hub", test_config())
            .expect("diagnostics should open");

        diagnostics
            .record(
                LogLevel::Warn,
                "route-fetch",
                "request failed token=hidden at https://example.com/private",
            )
            .expect("record should be written");

        let content =
            fs::read_to_string(temp.path().join("logs/app.log")).expect("log should be readable");
        let value: serde_json::Value =
            serde_json::from_str(content.trim()).expect("record should remain valid JSON");
        assert_eq!(value["level"], "warn");
        assert_eq!(value["target"], "route-fetch");
        assert!(!content.contains("hidden"));
        assert!(!content.contains("example.com"));
    }

    #[test]
    fn tracing_writer_redacts_structured_event_fields() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let diagnostics = Diagnostics::open(temp.path(), "wocao-hub", test_config())
            .expect("diagnostics should open");
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(diagnostics.tracing_writer()),
        );

        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(
                token = "secret-token",
                endpoint = "private-node:443",
                "route request failed"
            );
        });

        let content =
            fs::read_to_string(temp.path().join("logs/app.log")).expect("log should be readable");
        assert!(content.contains("route request failed"));
        assert!(!content.contains("secret-token"));
        assert!(!content.contains("private-node"));
    }

    #[test]
    fn exports_only_whitelisted_report_and_owned_logs() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let diagnostics = Diagnostics::open(temp.path(), "wocao-hub", test_config())
            .expect("diagnostics should open");
        diagnostics
            .record(
                LogLevel::Error,
                "proxy",
                "failed vless://user-secret@proxy.example.net:443?token=hidden",
            )
            .expect("record should be written");
        fs::write(
            temp.path().join("logs/recovery.json"),
            r#"{"proxy":"127.0.0.1:7890","password":"must-not-export"}"#,
        )
        .expect("unowned sensitive file should be written");
        fs::write(
            temp.path().join("recovery.json"),
            r#"{"networkState":"must-not-export"}"#,
        )
        .expect("recovery file should be written");

        let report = diagnostics
            .create_report(report_input())
            .expect("report should be created");
        let destination = temp.path().join("bundle.zip");
        let outcome = diagnostics
            .export_bundle(&destination, &report)
            .expect("bundle should export");

        assert_eq!(outcome.path(), destination);
        assert!(outcome.bytes() > 0);
        let file = fs::File::open(&destination).expect("archive should open");
        let mut archive = ZipArchive::new(file).expect("archive should parse");
        let mut names = Vec::new();
        let mut combined = String::new();
        let mut report_content = None;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("entry should open");
            let name = entry.name().to_owned();
            names.push(name.clone());
            let mut content = String::new();
            entry
                .read_to_string(&mut content)
                .expect("entry should be text");
            if name == "diagnostics.json" {
                report_content = Some(content.clone());
            }
            combined.push_str(&content);
        }

        assert!(names.contains(&"diagnostics.json".to_owned()));
        assert!(names.contains(&"logs/app.log".to_owned()));
        assert!(!names.iter().any(|name| name.contains("recovery")));
        let report_value: serde_json::Value = serde_json::from_str(
            report_content
                .as_deref()
                .expect("report entry should be present"),
        )
        .expect("redacted report should remain valid JSON");
        assert_eq!(report_value["schemaVersion"], 1);
        for secret in [
            "user-secret",
            "proxy.example.net",
            "hidden",
            "must-not-export",
            "127.0.0.1",
        ] {
            assert!(!combined.contains(secret), "secret survived: {secret}");
        }
    }

    #[test]
    fn refuses_to_overwrite_existing_export() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let diagnostics = Diagnostics::open(temp.path(), "wocao-hub", test_config())
            .expect("diagnostics should open");
        let report = diagnostics
            .create_report(report_input())
            .expect("report should be created");
        let destination = temp.path().join("bundle.zip");
        fs::write(&destination, "keep me").expect("existing file should be written");

        let error = diagnostics
            .export_bundle(&destination, &report)
            .expect_err("existing destination should be rejected");

        assert!(matches!(error, DiagnosticsError::DestinationExists));
        assert_eq!(
            fs::read_to_string(destination).expect("existing file should remain"),
            "keep me"
        );
    }

    #[test]
    fn validates_resource_limits() {
        let invalid = DiagnosticsConfig {
            max_files: 0,
            ..DiagnosticsConfig::default()
        };

        assert!(matches!(
            invalid.validate(),
            Err(DiagnosticsError::InvalidConfiguration("max_files"))
        ));
    }
}
