use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use crate::DiagnosticsError;

pub const DIAGNOSTIC_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OperatingSystem {
    Macos,
    Windows,
    Other,
}

impl OperatingSystem {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Architecture {
    Aarch64,
    X86_64,
    Other,
}

impl Architecture {
    pub const fn current() -> Self {
        if cfg!(target_arch = "aarch64") {
            Self::Aarch64
        } else if cfg!(target_arch = "x86_64") {
            Self::X86_64
        } else {
            Self::Other
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    pub const fn current() -> Self {
        if cfg!(debug_assertions) {
            Self::Debug
        } else {
            Self::Release
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckState {
    Healthy,
    Degraded,
    Failed,
    Unavailable,
    NotChecked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticChecks {
    pub desktop_app: CheckState,
    pub locale_configuration: CheckState,
    pub route_bundle: CheckState,
    pub local_proxy: CheckState,
    pub network_recovery: CheckState,
    pub official_downloads: CheckState,
}

impl Default for DiagnosticChecks {
    fn default() -> Self {
        Self {
            desktop_app: CheckState::NotChecked,
            locale_configuration: CheckState::NotChecked,
            route_bundle: CheckState::NotChecked,
            local_proxy: CheckState::NotChecked,
            network_recovery: CheckState::NotChecked,
            official_downloads: CheckState::NotChecked,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticReportInput {
    pub application_version: String,
    pub operating_system: OperatingSystem,
    pub architecture: Architecture,
    pub build_profile: BuildProfile,
    pub checks: DiagnosticChecks,
}

impl DiagnosticReportInput {
    pub fn for_current_platform(application_version: impl Into<String>) -> Self {
        Self {
            application_version: application_version.into(),
            operating_system: OperatingSystem::current(),
            architecture: Architecture::current(),
            build_profile: BuildProfile::current(),
            checks: DiagnosticChecks::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    schema_version: u32,
    generated_at: String,
    service_name: String,
    application_version: String,
    operating_system: OperatingSystem,
    architecture: Architecture,
    build_profile: BuildProfile,
    checks: DiagnosticChecks,
    retained_log_files: usize,
    retained_log_bytes: u64,
}

impl DiagnosticReport {
    pub(crate) fn create(
        service_name: &str,
        input: DiagnosticReportInput,
        retained_log_files: usize,
        retained_log_bytes: u64,
    ) -> Result<Self, DiagnosticsError> {
        validate_service_name(service_name)?;
        validate_application_version(&input.application_version)?;

        Ok(Self {
            schema_version: DIAGNOSTIC_REPORT_SCHEMA_VERSION,
            generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            service_name: service_name.to_owned(),
            application_version: input.application_version,
            operating_system: input.operating_system,
            architecture: input.architecture,
            build_profile: input.build_profile,
            checks: input.checks,
            retained_log_files,
            retained_log_bytes,
        })
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn generated_at(&self) -> &str {
        &self.generated_at
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn application_version(&self) -> &str {
        &self.application_version
    }

    pub const fn checks(&self) -> DiagnosticChecks {
        self.checks
    }
}

pub(crate) fn validate_service_name(value: &str) -> Result<(), DiagnosticsError> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || " ._-".contains(character))
    {
        return Err(DiagnosticsError::InvalidMetadata("service name"));
    }

    Ok(())
}

fn validate_application_version(value: &str) -> Result<(), DiagnosticsError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".+-_".contains(character))
    {
        return Err(DiagnosticsError::InvalidMetadata("application version"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_contains_only_whitelisted_fields() {
        let report = DiagnosticReport::create(
            "wocao-hub-desktop",
            DiagnosticReportInput {
                application_version: "1.2.3".to_owned(),
                operating_system: OperatingSystem::Macos,
                architecture: Architecture::Aarch64,
                build_profile: BuildProfile::Release,
                checks: DiagnosticChecks {
                    desktop_app: CheckState::Healthy,
                    locale_configuration: CheckState::Degraded,
                    ..DiagnosticChecks::default()
                },
            },
            2,
            512,
        )
        .expect("report should be created");

        let value = serde_json::to_value(report).expect("report should serialize");
        let object = value.as_object().expect("report should be an object");
        let expected = [
            "schemaVersion",
            "generatedAt",
            "serviceName",
            "applicationVersion",
            "operatingSystem",
            "architecture",
            "buildProfile",
            "checks",
            "retainedLogFiles",
            "retainedLogBytes",
        ];

        assert_eq!(object.len(), expected.len());
        for field in expected {
            assert!(object.contains_key(field), "missing field: {field}");
        }
    }

    #[test]
    fn rejects_metadata_that_could_smuggle_private_content() {
        let input =
            DiagnosticReportInput::for_current_platform("1.0.0 https://example.com?token=secret");

        let error = DiagnosticReport::create("wocao-hub", input, 0, 0)
            .expect_err("unsafe version should be rejected");

        assert!(matches!(
            error,
            DiagnosticsError::InvalidMetadata("application version")
        ));
    }
}
