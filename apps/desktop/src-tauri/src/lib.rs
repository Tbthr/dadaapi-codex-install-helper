pub mod activation_runtime;

use activation_core::{ActivationError, LocaleActivationService};
use activation_runtime::DesktopActivationState;
use shared_types::{
    ActivationEvent, ActivationPhase, CommandError, LocaleActivationResult, LocaleOverview,
    NetworkRecoveryStatus, OperatingSystem,
};
use tauri::{AppHandle, Emitter, Manager, State};

const ACTIVATION_PROGRESS_EVENT: &str = "activation-progress";

#[tauri::command]
async fn get_locale_overview() -> Result<LocaleOverview, CommandError> {
    LocaleActivationService::default()
        .overview()
        .await
        .map_err(command_error)
}

#[tauri::command]
fn is_activation_available(state: State<'_, DesktopActivationState>) -> bool {
    state.is_available()
}

#[tauri::command]
async fn activate_chinese(
    app: AppHandle,
    state: State<'_, DesktopActivationState>,
    selected_executable_path: Option<String>,
) -> Result<LocaleActivationResult, CommandError> {
    let runtime = state.runtime().ok_or_else(activation_unavailable_error)?;
    let event_app = app.clone();
    runtime
        .coordinator
        .activate_with_progress(selected_executable_path, move |phase| {
            let event = ActivationEvent::new(phase, activation_phase_message(phase));
            if event_app.emit(ACTIVATION_PROGRESS_EVENT, event).is_err() {
                tracing::warn!(?phase, "could not emit activation progress");
            }
        })
        .await
        .map_err(command_error)
}

#[tauri::command]
async fn get_network_recovery_status(
    state: State<'_, DesktopActivationState>,
) -> Result<NetworkRecoveryStatus, CommandError> {
    let Some(runtime) = state.runtime() else {
        return Ok(NetworkRecoveryStatus {
            pending: false,
            local_proxy_active: false,
        });
    };
    runtime
        .coordinator
        .network_recovery_status()
        .await
        .map_err(command_error)
}

#[tauri::command]
async fn restore_network(
    state: State<'_, DesktopActivationState>,
) -> Result<NetworkRecoveryStatus, CommandError> {
    let runtime = state.runtime().ok_or_else(activation_unavailable_error)?;
    runtime
        .coordinator
        .restore_network()
        .await
        .map_err(command_error)
}

fn activation_unavailable_error() -> CommandError {
    CommandError {
        code: "activation_unavailable".to_owned(),
        message: "当前版本未配置中文代理服务".to_owned(),
    }
}

fn activation_phase_message(phase: ActivationPhase) -> &'static str {
    match phase {
        ActivationPhase::Idle => "准备开始中文激活",
        ActivationPhase::DetectingApp => "正在重新检测 ChatGPT/Codex",
        ActivationPhase::FetchingProxyConfig => "正在获取并验证加密路由",
        ActivationPhase::FilteringProxyNodes => "正在筛选可用的海外节点",
        ActivationPhase::TestingProxyNodes => "正在并行检测候选节点（最多 15 秒）",
        ActivationPhase::SelectingProxyNode => "已选出当前最稳定节点",
        ActivationPhase::StartingLocalProxy => "正在启动临时本地代理",
        ActivationPhase::SavingNetworkState => "正在保存原网络设置",
        ActivationPhase::WritingLocale => "正在写入中文配置",
        ActivationPhase::StoppingDesktopApp => "正在关闭 ChatGPT/Codex",
        ActivationPhase::LaunchingDesktopApp => "正在通过最优节点启动应用",
        ActivationPhase::Verifying => "正在验证中文界面是否生效",
        ActivationPhase::RestoringNetwork => "正在恢复原网络设置",
        ActivationPhase::StoppingLocalProxy => "正在关闭临时本地代理",
        ActivationPhase::Succeeded => "中文已生效，代理仍在使用，请按需手动恢复网络",
        ActivationPhase::Failed => "中文激活未完成",
    }
}

fn command_error(error: ActivationError) -> CommandError {
    let code = match error {
        ActivationError::DesktopAppNotFound => "desktop_app_not_found",
        ActivationError::SelectedAppNotFound => "selected_app_not_found",
        ActivationError::Discovery(_) => "discovery_failed",
        ActivationError::Locale(_) => "locale_config_failed",
        ActivationError::Platform(_) => "platform_operation_failed",
        ActivationError::ProxyPreparation(_) => "proxy_preparation_failed",
        ActivationError::LocalProxy(_) => "local_proxy_failed",
        ActivationError::NetworkSafety(_) => "network_safety_failed",
        ActivationError::ChineseEffect(_) => "locale_verification_failed",
        ActivationError::InvalidTransition { .. } => "activation_state_failed",
        ActivationError::BackgroundTask(_) => "background_task_failed",
        ActivationError::VerificationFailed => "locale_verification_failed",
        ActivationError::PendingNetworkRecovery => "network_recovery_pending",
        ActivationError::LocalProxySessionMissing => "local_proxy_session_missing",
        ActivationError::OperationAndCleanupFailed { .. } => "activation_cleanup_failed",
    };
    CommandError {
        code: code.to_owned(),
        message: error.to_string(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diagnostics::init("wocao-hub-desktop");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let activation_runtime =
                activation_runtime::DesktopActivationRuntime::from_build_environment(app_data_dir)?;
            if activation_runtime.is_some() {
                tracing::info!("desktop activation runtime configured");
            } else {
                let configuration_names = activation_runtime::build_configuration_names();
                tracing::info!(
                    manifest_url = configuration_names[0],
                    public_key = configuration_names[1],
                    encryption_key = configuration_names[2],
                    key_id = configuration_names[3],
                    "desktop activation runtime not configured"
                );
            }
            app.manage(DesktopActivationState::new(activation_runtime));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_locale_overview,
            is_activation_available,
            activate_chinese,
            get_network_recovery_status,
            restore_network
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Wocao Hub desktop application");
}

fn current_operating_system() -> OperatingSystem {
    #[cfg(target_os = "macos")]
    {
        return OperatingSystem::MacOs;
    }

    #[cfg(target_os = "windows")]
    {
        return OperatingSystem::Windows;
    }

    #[allow(unreachable_code)]
    OperatingSystem::MacOs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_activation_phase_has_a_user_message() {
        for phase in [
            ActivationPhase::Idle,
            ActivationPhase::DetectingApp,
            ActivationPhase::FetchingProxyConfig,
            ActivationPhase::FilteringProxyNodes,
            ActivationPhase::TestingProxyNodes,
            ActivationPhase::SelectingProxyNode,
            ActivationPhase::StartingLocalProxy,
            ActivationPhase::SavingNetworkState,
            ActivationPhase::WritingLocale,
            ActivationPhase::StoppingDesktopApp,
            ActivationPhase::LaunchingDesktopApp,
            ActivationPhase::Verifying,
            ActivationPhase::RestoringNetwork,
            ActivationPhase::StoppingLocalProxy,
            ActivationPhase::Succeeded,
            ActivationPhase::Failed,
        ] {
            assert!(!activation_phase_message(phase).is_empty());
        }
    }

    #[test]
    fn unavailable_activation_returns_stable_command_error() {
        let error = activation_unavailable_error();

        assert_eq!(error.code, "activation_unavailable");
        assert!(!error.message.is_empty());
    }
}
