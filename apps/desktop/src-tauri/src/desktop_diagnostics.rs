use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use activation_core::LocaleActivationService;
use diagnostics::{
    CheckState, DiagnosticChecks, DiagnosticReport, DiagnosticReportInput, Diagnostics,
    DiagnosticsConfig, DiagnosticsError,
};
use downloader::catalog::official_download_catalog;
use serde::Serialize;
use shared_types::CommandError;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::activation_runtime::DesktopActivationState;

const SERVICE_NAME: &str = "dada-assistant-desktop";
const EXPORT_PREFIX: &str = "dada-assistant-diagnostics-";
const EXPORT_SUFFIX: &str = ".zip";

#[derive(Clone)]
pub struct DesktopDiagnosticsState {
    diagnostics: Diagnostics,
    export_directory: PathBuf,
}

impl DesktopDiagnosticsState {
    pub fn new(app_data_directory: &Path) -> Result<Self, DiagnosticsError> {
        let diagnostics = Diagnostics::open(
            app_data_directory,
            SERVICE_NAME,
            DiagnosticsConfig::default(),
        )?;
        let export_directory = app_data_directory.join("diagnostics-exports");
        ensure_private_directory(&export_directory).map_err(|source| DiagnosticsError::Io {
            operation: "create diagnostics export directory",
            source,
        })?;
        Ok(Self {
            diagnostics,
            export_directory,
        })
    }

    pub fn initialize_tracing(&self) -> Result<(), DiagnosticsError> {
        diagnostics::init_with_diagnostics(SERVICE_NAME, &self.diagnostics)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticExportResult {
    pub file_name: String,
    pub bytes: u64,
    pub entry_count: usize,
}

#[tauri::command]
pub async fn get_diagnostic_summary(
    diagnostics_state: State<'_, DesktopDiagnosticsState>,
    activation_state: State<'_, DesktopActivationState>,
) -> Result<DiagnosticReport, CommandError> {
    create_diagnostic_report(&diagnostics_state, &activation_state).await
}

#[tauri::command]
pub async fn export_diagnostics(
    diagnostics_state: State<'_, DesktopDiagnosticsState>,
    activation_state: State<'_, DesktopActivationState>,
) -> Result<DiagnosticExportResult, CommandError> {
    let report = create_diagnostic_report(&diagnostics_state, &activation_state).await?;
    let file_name = diagnostic_export_file_name();
    let destination = diagnostics_state.export_directory.join(&file_name);
    let diagnostics = diagnostics_state.diagnostics.clone();
    let outcome =
        tokio::task::spawn_blocking(move || diagnostics.export_bundle(&destination, &report))
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "diagnostics export worker failed");
                diagnostic_command_error("diagnostics_export_failed", "无法导出诊断信息")
            })?
            .map_err(map_diagnostics_error)?;

    Ok(DiagnosticExportResult {
        file_name,
        bytes: outcome.bytes(),
        entry_count: outcome.entry_count(),
    })
}

#[tauri::command]
pub fn reveal_diagnostics_export(
    app: AppHandle,
    diagnostics_state: State<'_, DesktopDiagnosticsState>,
    file_name: String,
) -> Result<(), CommandError> {
    if !valid_export_file_name(&file_name) {
        return Err(diagnostic_command_error(
            "diagnostics_export_not_found",
            "诊断文件不存在",
        ));
    }
    ensure_private_directory(&diagnostics_state.export_directory).map_err(|error| {
        tracing::warn!(error = %error, "diagnostics export directory is unavailable");
        diagnostic_command_error("diagnostics_export_not_found", "诊断文件不存在")
    })?;
    let path = diagnostics_state.export_directory.join(file_name);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        tracing::warn!(error = %error, "diagnostics export is unavailable");
        diagnostic_command_error("diagnostics_export_not_found", "诊断文件不存在")
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(diagnostic_command_error(
            "diagnostics_export_not_found",
            "诊断文件不存在",
        ));
    }

    app.opener().reveal_item_in_dir(path).map_err(|error| {
        tracing::warn!(error = %error, "could not reveal diagnostics export");
        diagnostic_command_error("diagnostics_reveal_failed", "无法打开诊断文件所在目录")
    })
}

async fn create_diagnostic_report(
    diagnostics_state: &DesktopDiagnosticsState,
    activation_state: &DesktopActivationState,
) -> Result<DiagnosticReport, CommandError> {
    let locale_overview = LocaleActivationService::default().overview().await;
    let (desktop_app, locale_configuration) = match locale_overview {
        Ok(overview) => (
            if overview.apps.is_empty() {
                CheckState::Unavailable
            } else {
                CheckState::Healthy
            },
            if overview.locale.chinese_enabled {
                CheckState::Healthy
            } else {
                CheckState::Degraded
            },
        ),
        Err(error) => {
            tracing::warn!(error = %error, "diagnostic locale inspection failed");
            (CheckState::Failed, CheckState::Failed)
        }
    };

    let (network_recovery, local_proxy) =
        match super::network_recovery_status(activation_state).await {
            Ok(status) => (
                if status.pending {
                    CheckState::Degraded
                } else {
                    CheckState::Healthy
                },
                if status.local_proxy_active {
                    CheckState::Healthy
                } else if status.pending {
                    CheckState::Degraded
                } else {
                    CheckState::Healthy
                },
            ),
            Err(error) => {
                tracing::warn!(code = %error.code, "diagnostic network inspection failed");
                (CheckState::Failed, CheckState::Failed)
            }
        };

    let official_downloads = if official_download_catalog(
        super::current_operating_system(),
        super::current_cpu_architecture(),
    )
    .is_ok()
    {
        CheckState::Healthy
    } else {
        CheckState::Failed
    };

    let input = DiagnosticReportInput {
        application_version: env!("CARGO_PKG_VERSION").to_owned(),
        operating_system: diagnostics::OperatingSystem::current(),
        architecture: diagnostics::Architecture::current(),
        build_profile: diagnostics::BuildProfile::current(),
        checks: DiagnosticChecks {
            desktop_app,
            locale_configuration,
            route_bundle: if activation_state.is_available() {
                CheckState::Healthy
            } else {
                CheckState::Unavailable
            },
            local_proxy,
            network_recovery,
            official_downloads,
        },
    };
    diagnostics_state
        .diagnostics
        .create_report(input)
        .map_err(map_diagnostics_error)
}

fn diagnostic_export_file_name() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(
        "{EXPORT_PREFIX}{timestamp}-{}{EXPORT_SUFFIX}",
        Uuid::new_v4().simple()
    )
}

fn valid_export_file_name(file_name: &str) -> bool {
    !file_name.is_empty()
        && Path::new(file_name)
            .file_name()
            .is_some_and(|name| name == file_name)
        && file_name.starts_with(EXPORT_PREFIX)
        && file_name.ends_with(EXPORT_SUFFIX)
        && file_name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.'))
}

fn map_diagnostics_error(error: DiagnosticsError) -> CommandError {
    tracing::warn!(error = %error, "diagnostics operation failed");
    match error {
        DiagnosticsError::DestinationExists => diagnostic_command_error(
            "diagnostics_export_conflict",
            "诊断文件名发生冲突，请重新导出",
        ),
        DiagnosticsError::SensitiveDataDetected => diagnostic_command_error(
            "diagnostics_redaction_failed",
            "诊断信息脱敏校验未通过，已停止导出",
        ),
        DiagnosticsError::ExportSizeLimitExceeded | DiagnosticsError::LogSizeLimitExceeded => {
            diagnostic_command_error("diagnostics_export_too_large", "诊断信息超过安全大小限制")
        }
        _ => diagnostic_command_error("diagnostics_failed", "诊断功能暂时不可用"),
    }
}

fn diagnostic_command_error(code: &str, message: &str) -> CommandError {
    CommandError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), io::Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsafe diagnostics export directory",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(path)?,
        Err(error) => return Err(error),
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_file_name_rejects_path_traversal_and_untrusted_names() {
        assert!(valid_export_file_name(
            "dada-assistant-diagnostics-123-abc.zip"
        ));
        for value in [
            "../dada-assistant-diagnostics-123.zip",
            "dada-assistant-diagnostics-123.zip/other",
            "other.zip",
            "dada-assistant-diagnostics-123.exe",
        ] {
            assert!(!valid_export_file_name(value));
        }
    }

    #[test]
    fn export_directory_is_private_on_unix() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("exports");
        ensure_private_directory(&path).expect("create export directory");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(path)
                .expect("read export directory")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }
    }
}
