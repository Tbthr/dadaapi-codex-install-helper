use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use downloader::{
    catalog::{
        official_download_catalog, resolve_official_artifact, CatalogError, TrustedDownloadArtifact,
    },
    DownloadCancellation, DownloadClient, DownloadError, DownloadProgress, DownloadRequest,
};
use serde::{Deserialize, Serialize};
use shared_types::{
    CommandError, DownloadCatalog, DownloadTaskSnapshot, DownloadTaskState, SoftwareProductId,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

pub const DOWNLOAD_TASK_UPDATED_EVENT: &str = "download-task-updated";
const PROGRESS_EMIT_BYTES: u64 = 256 * 1024;

#[derive(Debug)]
struct DownloadTaskRecord {
    snapshot: DownloadTaskSnapshot,
    target_path: PathBuf,
    cancellation: DownloadCancellation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDownloadTask {
    snapshot: DownloadTaskSnapshot,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDownloads {
    tasks: Vec<PersistedDownloadTask>,
}

pub struct DesktopDownloadState {
    tasks: Mutex<HashMap<Uuid, DownloadTaskRecord>>,
    download_dir: PathBuf,
    state_path: PathBuf,
}

impl DesktopDownloadState {
    pub fn new(download_dir: PathBuf, state_path: PathBuf) -> Result<Self, io::Error> {
        ensure_private_directory(&download_dir)?;
        let tasks = match load_persisted_tasks(
            &state_path,
            &download_dir,
            super::current_operating_system(),
            super::current_cpu_architecture(),
        ) {
            Ok(tasks) => tasks,
            Err(error) => {
                tracing::warn!(error = %error, "could not load persisted download tasks");
                HashMap::new()
            }
        };
        Ok(Self {
            tasks: Mutex::new(tasks),
            download_dir,
            state_path,
        })
    }
}

#[tauri::command]
pub fn get_download_catalog() -> Result<DownloadCatalog, CommandError> {
    official_download_catalog(
        super::current_operating_system(),
        super::current_cpu_architecture(),
    )
    .map_err(catalog_command_error)
}

#[tauri::command]
pub async fn get_official_download_link(
    product_id: SoftwareProductId,
    artifact_id: Option<String>,
) -> Result<String, CommandError> {
    let artifact = resolve_official_artifact(
        product_id,
        artifact_id.as_deref(),
        super::current_operating_system(),
        super::current_cpu_architecture(),
    )
    .map_err(catalog_command_error)?;
    artifact
        .resolve_source_url()
        .await
        .map(|url| url.to_string())
        .map_err(catalog_command_error)
}

#[tauri::command]
pub async fn list_download_tasks(
    state: State<'_, DesktopDownloadState>,
) -> Result<Vec<DownloadTaskSnapshot>, CommandError> {
    let mut snapshots = state
        .tasks
        .lock()
        .await
        .values()
        .map(|record| record.snapshot.clone())
        .collect::<Vec<_>>();
    snapshots.sort_by_key(|snapshot| snapshot.id);
    Ok(snapshots)
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    state: State<'_, DesktopDownloadState>,
    product_id: SoftwareProductId,
    artifact_id: Option<String>,
) -> Result<DownloadTaskSnapshot, CommandError> {
    let artifact = resolve_official_artifact(
        product_id,
        artifact_id.as_deref(),
        super::current_operating_system(),
        super::current_cpu_architecture(),
    )
    .map_err(catalog_command_error)?;

    let mut tasks = state.tasks.lock().await;
    if let Some(existing) = tasks.values().find(|record| {
        record.snapshot.product_id == product_id
            && record.snapshot.artifact_id == artifact.summary.id
    }) {
        return Ok(existing.snapshot.clone());
    }

    let id = Uuid::new_v4();
    let target_path = prepare_download_target(&state.download_dir, &artifact)
        .map_err(download_directory_command_error)?;
    let snapshot = DownloadTaskSnapshot {
        id,
        product_id,
        artifact_id: artifact.summary.id.clone(),
        state: DownloadTaskState::Queued,
        downloaded_bytes: 0,
        total_bytes: None,
        resumed_from: 0,
        file_name: artifact.summary.file_name.clone(),
        error: None,
    };
    tasks.insert(
        id,
        DownloadTaskRecord {
            snapshot: snapshot.clone(),
            target_path: target_path.clone(),
            cancellation: DownloadCancellation::new(),
        },
    );
    if let Err(error) = persist_tasks(&state.state_path, &tasks) {
        tasks.remove(&id);
        return Err(persistence_command_error(error));
    }
    drop(tasks);
    emit_snapshot(&app, &snapshot);
    spawn_download(app, id, artifact, target_path);
    Ok(snapshot)
}

#[tauri::command]
pub async fn cancel_download(
    app: AppHandle,
    state: State<'_, DesktopDownloadState>,
    task_id: Uuid,
) -> Result<DownloadTaskSnapshot, CommandError> {
    let mut tasks = state.tasks.lock().await;
    let record = tasks
        .get_mut(&task_id)
        .ok_or_else(|| task_command_error("download_task_not_found", "下载任务不存在"))?;
    let previous_snapshot = record.snapshot.clone();
    let cancellation = record.cancellation.clone();
    if task_is_cancellable(record.snapshot.state) {
        record.snapshot.state = DownloadTaskState::Cancelled;
        record.snapshot.error = None;
    }
    let snapshot = record.snapshot.clone();
    if let Err(error) = persist_tasks(&state.state_path, &tasks) {
        if let Some(record) = tasks.get_mut(&task_id) {
            record.snapshot = previous_snapshot;
        }
        return Err(persistence_command_error(error));
    }
    if snapshot.state == DownloadTaskState::Cancelled {
        cancellation.cancel();
    }
    drop(tasks);
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn retry_download(
    app: AppHandle,
    state: State<'_, DesktopDownloadState>,
    task_id: Uuid,
) -> Result<DownloadTaskSnapshot, CommandError> {
    let mut tasks = state.tasks.lock().await;
    let current = tasks
        .get(&task_id)
        .ok_or_else(|| task_command_error("download_task_not_found", "下载任务不存在"))?;
    if !matches!(
        current.snapshot.state,
        DownloadTaskState::Cancelled | DownloadTaskState::Failed
    ) {
        return Ok(current.snapshot.clone());
    }
    if let Some(other) = tasks.values().find(|record| {
        record.snapshot.id != task_id
            && record.snapshot.product_id == current.snapshot.product_id
            && record.snapshot.artifact_id == current.snapshot.artifact_id
    }) {
        return Ok(other.snapshot.clone());
    }

    let artifact = resolve_official_artifact(
        current.snapshot.product_id,
        Some(&current.snapshot.artifact_id),
        super::current_operating_system(),
        super::current_cpu_architecture(),
    )
    .map_err(catalog_command_error)?;
    let target_path = prepare_download_target(&state.download_dir, &artifact)
        .map_err(download_directory_command_error)?;
    let record = tasks
        .get_mut(&task_id)
        .ok_or_else(|| task_command_error("download_task_not_found", "下载任务不存在"))?;
    let previous_snapshot = record.snapshot.clone();
    let previous_target_path = record.target_path.clone();
    let previous_cancellation = record.cancellation.clone();
    record.target_path = target_path.clone();
    record.cancellation = DownloadCancellation::new();
    record.snapshot.state = if target_path.is_file() {
        DownloadTaskState::Ready
    } else {
        DownloadTaskState::Queued
    };
    record.snapshot.error = None;
    let snapshot = record.snapshot.clone();
    if let Err(error) = persist_tasks(&state.state_path, &tasks) {
        if let Some(record) = tasks.get_mut(&task_id) {
            record.snapshot = previous_snapshot;
            record.target_path = previous_target_path;
            record.cancellation = previous_cancellation;
        }
        return Err(persistence_command_error(error));
    }
    drop(tasks);
    emit_snapshot(&app, &snapshot);
    if snapshot.state == DownloadTaskState::Queued {
        spawn_download(app, task_id, artifact, target_path);
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn reveal_download(
    app: AppHandle,
    state: State<'_, DesktopDownloadState>,
    task_id: Uuid,
) -> Result<(), CommandError> {
    let target_path = {
        let tasks = state.tasks.lock().await;
        let record = tasks
            .get(&task_id)
            .ok_or_else(|| task_command_error("download_task_not_found", "下载任务不存在"))?;
        if !matches!(
            record.snapshot.state,
            DownloadTaskState::Ready | DownloadTaskState::Launched
        ) {
            return Err(task_command_error(
                "download_not_ready",
                "安装包尚未下载完成",
            ));
        }
        let artifact = resolve_official_artifact(
            record.snapshot.product_id,
            Some(&record.snapshot.artifact_id),
            super::current_operating_system(),
            super::current_cpu_architecture(),
        )
        .map_err(catalog_command_error)?;
        prepare_download_target(&state.download_dir, &artifact)
            .map_err(download_directory_command_error)?
    };
    if !target_path.is_file() {
        return Err(task_command_error(
            "download_file_missing",
            "已下载的安装包不存在",
        ));
    }
    app.opener()
        .reveal_item_in_dir(target_path)
        .map_err(|error| {
            tracing::warn!(error = %error, "could not reveal downloaded installer");
            task_command_error("download_reveal_failed", "无法打开安装包所在目录")
        })
}

#[tauri::command]
pub async fn launch_installer(
    app: AppHandle,
    state: State<'_, DesktopDownloadState>,
    task_id: Uuid,
) -> Result<DownloadTaskSnapshot, CommandError> {
    let (target_path, launching_snapshot) = {
        let mut tasks = state.tasks.lock().await;
        let record = tasks
            .get_mut(&task_id)
            .ok_or_else(|| task_command_error("download_task_not_found", "下载任务不存在"))?;
        if !matches!(
            record.snapshot.state,
            DownloadTaskState::Ready | DownloadTaskState::Launched
        ) {
            return Err(task_command_error(
                "download_not_ready",
                "安装包尚未下载完成",
            ));
        }
        let artifact = resolve_official_artifact(
            record.snapshot.product_id,
            Some(&record.snapshot.artifact_id),
            super::current_operating_system(),
            super::current_cpu_architecture(),
        )
        .map_err(catalog_command_error)?;
        let target_path = prepare_download_target(&state.download_dir, &artifact)
            .map_err(download_directory_command_error)?;
        if !target_path.is_file() {
            return Err(task_command_error(
                "download_file_missing",
                "已下载的安装包不存在",
            ));
        }
        record.target_path = target_path.clone();
        record.snapshot.state = DownloadTaskState::Launching;
        record.snapshot.error = None;
        let snapshot = record.snapshot.clone();
        persist_tasks(&state.state_path, &tasks).map_err(persistence_command_error)?;
        (target_path, snapshot)
    };
    emit_snapshot(&app, &launching_snapshot);

    if let Err(error) = app
        .opener()
        .open_path(target_path.to_string_lossy().into_owned(), None::<&str>)
    {
        tracing::warn!(error = %error, "could not open downloaded installer");
        let command_error = task_command_error("installer_launch_failed", "无法打开安装包");
        update_task_state(
            &app,
            task_id,
            DownloadTaskState::Ready,
            Some(command_error.clone()),
        )
        .await;
        return Err(command_error);
    }

    update_task_state(&app, task_id, DownloadTaskState::Launched, None)
        .await
        .ok_or_else(|| task_command_error("download_task_not_found", "下载任务不存在"))
}

#[tauri::command]
pub fn open_official_product_page(
    app: AppHandle,
    product_id: SoftwareProductId,
) -> Result<(), CommandError> {
    let catalog = official_download_catalog(
        super::current_operating_system(),
        super::current_cpu_architecture(),
    )
    .map_err(catalog_command_error)?;
    let product = catalog
        .products
        .into_iter()
        .find(|product| product.id == product_id)
        .ok_or_else(|| task_command_error("download_product_not_found", "官方软件产品不存在"))?;
    app.opener()
        .open_url(product.official_page_url.as_str(), None::<&str>)
        .map_err(|error| {
            tracing::warn!(error = %error, "could not open official product page");
            task_command_error("official_page_open_failed", "无法打开官方下载页面")
        })
}

fn spawn_download(
    app: AppHandle,
    task_id: Uuid,
    artifact: TrustedDownloadArtifact,
    target_path: PathBuf,
) {
    tauri::async_runtime::spawn(async move {
        run_download(app, task_id, artifact, target_path).await;
    });
}

fn prepare_download_target(
    download_directory: &Path,
    artifact: &TrustedDownloadArtifact,
) -> Result<PathBuf, io::Error> {
    if !is_safe_path_component(&artifact.summary.id)
        || !is_safe_path_component(&artifact.summary.file_name)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsafe trusted download path component",
        ));
    }
    ensure_private_directory(download_directory)?;
    let artifact_directory = download_directory.join(&artifact.summary.id);
    ensure_private_directory(&artifact_directory)?;
    Ok(artifact_directory.join(&artifact.summary.file_name))
}

fn is_safe_path_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn ensure_private_directory(path: &Path) -> Result<(), io::Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsafe download directory",
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

async fn run_download(
    app: AppHandle,
    task_id: Uuid,
    artifact: TrustedDownloadArtifact,
    target_path: PathBuf,
) {
    if update_task_state(&app, task_id, DownloadTaskState::Resolving, None)
        .await
        .is_none()
    {
        return;
    }

    let client = match artifact.download_client() {
        Ok(client) => DownloadClient::new(client),
        Err(error) => {
            fail_task(&app, task_id, catalog_command_error(error)).await;
            return;
        }
    };
    let source_url = match artifact.resolve_source_url().await {
        Ok(source_url) => source_url,
        Err(error) => {
            fail_task(&app, task_id, catalog_command_error(error)).await;
            return;
        }
    };
    let total_bytes = match client.content_length(source_url.clone()).await {
        Ok(Some(total_bytes)) if total_bytes <= artifact.maximum_size_bytes => total_bytes,
        Ok(Some(_)) => {
            fail_task(
                &app,
                task_id,
                task_command_error("download_too_large", "官方安装包超过允许的大小"),
            )
            .await;
            return;
        }
        Ok(None) => {
            fail_task(
                &app,
                task_id,
                task_command_error("download_size_unknown", "无法确认官方安装包大小"),
            )
            .await;
            return;
        }
        Err(error) => {
            fail_task(&app, task_id, download_command_error(error)).await;
            return;
        }
    };

    let cancellation = {
        let state = app.state::<DesktopDownloadState>();
        let mut tasks = state.tasks.lock().await;
        let Some(record) = tasks.get_mut(&task_id) else {
            return;
        };
        if record.snapshot.state == DownloadTaskState::Cancelled {
            return;
        }
        record.snapshot.state = DownloadTaskState::Downloading;
        record.snapshot.total_bytes = Some(total_bytes);
        record.snapshot.error = None;
        let snapshot = record.snapshot.clone();
        let cancellation = record.cancellation.clone();
        if let Err(error) = persist_tasks(&state.state_path, &tasks) {
            drop(tasks);
            fail_task(&app, task_id, persistence_command_error(error)).await;
            return;
        }
        drop(tasks);
        emit_snapshot(&app, &snapshot);
        cancellation
    };

    let request = DownloadRequest::new(source_url, target_path.clone())
        .with_expected_length(total_bytes)
        .with_cancellation(cancellation.clone());
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<DownloadProgress>();
    let oversize = Arc::new(AtomicBool::new(false));
    let oversize_callback = oversize.clone();
    let callback_cancellation = cancellation.clone();
    let maximum_size = artifact.maximum_size_bytes;
    let download_future = client.download_with_progress(request, move |progress| {
        if progress.downloaded_bytes > maximum_size {
            oversize_callback.store(true, Ordering::Release);
            callback_cancellation.cancel();
        }
        let _ = progress_tx.send(progress);
    });
    tokio::pin!(download_future);
    let mut last_emitted_bytes = 0_u64;

    let result = loop {
        tokio::select! {
            result = &mut download_future => break result,
            progress = progress_rx.recv() => {
                let Some(progress) = progress else {
                    continue;
                };
                let is_final = progress.total_bytes == Some(progress.downloaded_bytes);
                if is_final
                    || progress.downloaded_bytes.saturating_sub(last_emitted_bytes)
                        >= PROGRESS_EMIT_BYTES
                {
                    last_emitted_bytes = progress.downloaded_bytes;
                    update_task_progress(&app, task_id, progress).await;
                }
            }
        }
    };

    match result {
        Ok(outcome) => {
            let state = app.state::<DesktopDownloadState>();
            let mut tasks = state.tasks.lock().await;
            let Some(record) = tasks.get_mut(&task_id) else {
                return;
            };
            record.snapshot.state = DownloadTaskState::Ready;
            record.snapshot.downloaded_bytes = outcome.total_bytes;
            record.snapshot.total_bytes = Some(outcome.total_bytes);
            record.snapshot.resumed_from = outcome.resumed_from;
            record.snapshot.error = None;
            let snapshot = record.snapshot.clone();
            if let Err(error) = persist_tasks(&state.state_path, &tasks) {
                tracing::warn!(error = %error, "could not persist completed download task");
            }
            drop(tasks);
            emit_snapshot(&app, &snapshot);
        }
        Err(DownloadError::Cancelled) if !oversize.load(Ordering::Acquire) => {
            update_task_state(&app, task_id, DownloadTaskState::Cancelled, None).await;
        }
        Err(_) if oversize.load(Ordering::Acquire) => {
            fail_task(
                &app,
                task_id,
                task_command_error("download_too_large", "官方安装包超过允许的大小"),
            )
            .await;
        }
        Err(error) => fail_task(&app, task_id, download_command_error(error)).await,
    }
}

async fn update_task_progress(app: &AppHandle, task_id: Uuid, progress: DownloadProgress) {
    let state = app.state::<DesktopDownloadState>();
    let mut tasks = state.tasks.lock().await;
    let Some(record) = tasks.get_mut(&task_id) else {
        return;
    };
    if record.snapshot.state != DownloadTaskState::Downloading {
        return;
    }
    record.snapshot.downloaded_bytes = progress.downloaded_bytes;
    record.snapshot.total_bytes = progress.total_bytes;
    record.snapshot.resumed_from = progress.resumed_from;
    let snapshot = record.snapshot.clone();
    drop(tasks);
    emit_snapshot(app, &snapshot);
}

async fn update_task_state(
    app: &AppHandle,
    task_id: Uuid,
    task_state: DownloadTaskState,
    error: Option<CommandError>,
) -> Option<DownloadTaskSnapshot> {
    let state = app.state::<DesktopDownloadState>();
    let mut tasks = state.tasks.lock().await;
    let record = tasks.get_mut(&task_id)?;
    record.snapshot.state = task_state;
    record.snapshot.error = error;
    let snapshot = record.snapshot.clone();
    if let Err(persist_error) = persist_tasks(&state.state_path, &tasks) {
        tracing::warn!(error = %persist_error, "could not persist download task state");
    }
    drop(tasks);
    emit_snapshot(app, &snapshot);
    Some(snapshot)
}

async fn fail_task(app: &AppHandle, task_id: Uuid, error: CommandError) {
    update_task_state(app, task_id, DownloadTaskState::Failed, Some(error)).await;
}

fn emit_snapshot(app: &AppHandle, snapshot: &DownloadTaskSnapshot) {
    if app.emit(DOWNLOAD_TASK_UPDATED_EVENT, snapshot).is_err() {
        tracing::warn!(task_id = %snapshot.id, "could not emit download task update");
    }
}

fn task_is_active(state: DownloadTaskState) -> bool {
    matches!(
        state,
        DownloadTaskState::Queued
            | DownloadTaskState::Resolving
            | DownloadTaskState::Downloading
            | DownloadTaskState::Launching
    )
}

fn task_is_cancellable(state: DownloadTaskState) -> bool {
    matches!(
        state,
        DownloadTaskState::Queued | DownloadTaskState::Resolving | DownloadTaskState::Downloading
    )
}

fn load_persisted_tasks(
    state_path: &Path,
    download_dir: &Path,
    operating_system: shared_types::OperatingSystem,
    cpu_architecture: shared_types::CpuArchitecture,
) -> Result<HashMap<Uuid, DownloadTaskRecord>, io::Error> {
    let payload = match fs::read(state_path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(error),
    };
    let persisted: PersistedDownloads = serde_json::from_slice(&payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    persisted
        .tasks
        .into_iter()
        .map(|task| {
            let mut snapshot = task.snapshot;
            let artifact = resolve_official_artifact(
                snapshot.product_id,
                Some(&snapshot.artifact_id),
                operating_system,
                cpu_architecture,
            )
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            snapshot.file_name = artifact.summary.file_name.clone();
            if task_is_active(snapshot.state) {
                snapshot.state = DownloadTaskState::Failed;
                snapshot.error = Some(task_command_error(
                    "download_interrupted",
                    "下载在应用退出时中断，可以继续重试",
                ));
            }
            Ok((
                snapshot.id,
                DownloadTaskRecord {
                    snapshot,
                    target_path: prepare_download_target(download_dir, &artifact)?,
                    cancellation: DownloadCancellation::new(),
                },
            ))
        })
        .collect()
}

fn persist_tasks(
    state_path: &Path,
    tasks: &HashMap<Uuid, DownloadTaskRecord>,
) -> Result<(), io::Error> {
    let parent = state_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let persisted = PersistedDownloads {
        tasks: tasks
            .values()
            .map(|record| PersistedDownloadTask {
                snapshot: record.snapshot.clone(),
            })
            .collect(),
    };
    let payload = serde_json::to_vec_pretty(&persisted)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temporary_path = state_path.with_extension("json.tmp");
    write_private_file(&temporary_path, &payload)?;
    #[cfg(target_os = "windows")]
    if state_path.exists() {
        fs::remove_file(state_path)?;
    }
    fs::rename(&temporary_path, state_path)?;
    Ok(())
}

fn write_private_file(path: &Path, payload: &[u8]) -> Result<(), io::Error> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(path)?;
    file.write_all(payload)?;
    file.sync_all()
}

fn catalog_command_error(error: CatalogError) -> CommandError {
    task_command_error("download_catalog_failed", &error.to_string())
}

fn download_command_error(error: DownloadError) -> CommandError {
    let code = match error {
        DownloadError::Cancelled => "download_cancelled",
        DownloadError::ChecksumMismatch { .. } => "download_checksum_failed",
        DownloadError::TargetAlreadyExists => "download_target_exists",
        DownloadError::UnexpectedStatus(_) => "download_http_failed",
        _ => "download_failed",
    };
    task_command_error(code, &error.to_string())
}

fn persistence_command_error(error: io::Error) -> CommandError {
    tracing::warn!(error = %error, "download state persistence failed");
    task_command_error("download_state_failed", "无法保存下载任务状态")
}

fn download_directory_command_error(error: io::Error) -> CommandError {
    tracing::warn!(error = %error, "download directory is unavailable");
    task_command_error("download_directory_failed", "无法安全使用下载目录")
}

fn task_command_error(code: &str, message: &str) -> CommandError {
    CommandError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn persisted_active_task_becomes_retryable_failure() {
        let directory = TempDir::new().expect("temporary directory");
        let state_path = directory.path().join("downloads.json");
        let download_dir = directory.path().join("downloads");
        let task_id = Uuid::new_v4();
        let artifact = resolve_official_artifact(
            SoftwareProductId::ChatGptDesktop,
            None,
            super::super::current_operating_system(),
            super::super::current_cpu_architecture(),
        )
        .expect("host artifact");
        let persisted = PersistedDownloads {
            tasks: vec![PersistedDownloadTask {
                snapshot: DownloadTaskSnapshot {
                    id: task_id,
                    product_id: SoftwareProductId::ChatGptDesktop,
                    artifact_id: artifact.summary.id.clone(),
                    state: DownloadTaskState::Downloading,
                    downloaded_bytes: 128,
                    total_bytes: Some(256),
                    resumed_from: 0,
                    file_name: "tampered-name.exe".to_owned(),
                    error: None,
                },
            }],
        };
        let mut persisted_value = serde_json::to_value(&persisted).expect("serialize state");
        persisted_value["tasks"][0]["targetPath"] =
            serde_json::Value::String("/tmp/attacker-selected-installer.exe".to_owned());
        fs::write(
            &state_path,
            serde_json::to_vec(&persisted_value).expect("encode state"),
        )
        .expect("write state");

        let tasks = load_persisted_tasks(
            &state_path,
            &download_dir,
            super::super::current_operating_system(),
            super::super::current_cpu_architecture(),
        )
        .expect("load tasks");
        let record = &tasks[&task_id];
        let snapshot = &record.snapshot;
        assert_eq!(snapshot.state, DownloadTaskState::Failed);
        assert_eq!(snapshot.file_name, artifact.summary.file_name);
        assert_eq!(
            record.target_path,
            download_dir
                .join(&artifact.summary.id)
                .join(&artifact.summary.file_name)
        );
        assert_eq!(
            snapshot.error.as_ref().map(|error| error.code.as_str()),
            Some("download_interrupted")
        );
    }

    #[test]
    fn persisted_download_state_does_not_store_source_url() {
        let directory = TempDir::new().expect("temporary directory");
        let state_path = directory.path().join("downloads.json");
        let task_id = Uuid::new_v4();
        let mut tasks = HashMap::new();
        tasks.insert(
            task_id,
            DownloadTaskRecord {
                snapshot: DownloadTaskSnapshot {
                    id: task_id,
                    product_id: SoftwareProductId::ChatGptDesktop,
                    artifact_id: "chatgpt-windows-arm64-msix".to_owned(),
                    state: DownloadTaskState::Queued,
                    downloaded_bytes: 0,
                    total_bytes: None,
                    resumed_from: 0,
                    file_name: "ChatGPT.msix".to_owned(),
                    error: None,
                },
                target_path: directory.path().join("ChatGPT.msix"),
                cancellation: DownloadCancellation::new(),
            },
        );

        persist_tasks(&state_path, &tasks).expect("persist tasks");
        let payload = fs::read_to_string(state_path).expect("read state");
        assert!(!payload.contains("https://"));
        assert!(!payload.contains("get.microsoft.com"));
        assert!(!payload.contains("targetPath"));
        assert!(!payload.contains(directory.path().to_string_lossy().as_ref()));
    }
}
