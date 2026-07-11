use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use tracing_subscriber::fmt::MakeWriter;

use crate::{DiagnosticsConfig, DiagnosticsError, Redactor};

const CURRENT_LOG_NAME: &str = "app.log";
const TRUNCATION_MARKER: &str = " [TRUNCATED]";

#[derive(Clone)]
pub(crate) struct LogStore {
    inner: Arc<LogStoreInner>,
}

struct LogStoreInner {
    directory: PathBuf,
    config: DiagnosticsConfig,
    redactor: Redactor,
    lock: Mutex<()>,
}

pub(crate) struct LogStats {
    pub file_count: usize,
    pub total_bytes: u64,
}

pub(crate) struct LogSnapshot {
    pub archive_name: String,
    pub content: Vec<u8>,
}

impl LogStore {
    pub fn open(
        data_directory: &Path,
        config: DiagnosticsConfig,
        redactor: Redactor,
    ) -> Result<Self, DiagnosticsError> {
        ensure_directory(data_directory, false)?;
        let directory = data_directory.join("logs");
        ensure_directory(&directory, true)?;

        let store = Self {
            inner: Arc::new(LogStoreInner {
                directory,
                config,
                redactor,
                lock: Mutex::new(()),
            }),
        };
        store.prune()?;
        Ok(store)
    }

    pub fn append(&self, input: &str) -> Result<(), DiagnosticsError> {
        let redacted = self.inner.redactor.redact(input);
        let record = bounded_record(&redacted, self.inner.config.max_record_bytes);
        let _guard = self.lock()?;
        self.prune_locked()?;

        let current_path = self.inner.directory.join(CURRENT_LOG_NAME);
        let current_length = safe_file_length(&current_path)?.unwrap_or(0);
        let record_length = u64::try_from(record.len())
            .map_err(|_| DiagnosticsError::InvalidConfiguration("record size"))?;
        if current_length > 0
            && current_length.saturating_add(record_length) > self.inner.config.max_file_bytes
        {
            self.rotate_locked()?;
        }

        let mut file = open_private_append_file(&current_path)?;
        file.write_all(&record)
            .map_err(|source| DiagnosticsError::io("write log", source))?;
        file.flush()
            .map_err(|source| DiagnosticsError::io("flush log", source))?;
        Ok(())
    }

    pub fn prune(&self) -> Result<(), DiagnosticsError> {
        let _guard = self.lock()?;
        self.prune_locked()
    }

    pub fn stats(&self) -> Result<LogStats, DiagnosticsError> {
        let _guard = self.lock()?;
        self.prune_locked()?;

        let mut file_count = 0usize;
        let mut total_bytes = 0u64;
        for path in self.retained_log_paths() {
            if let Some(length) = safe_file_length(&path)? {
                file_count = file_count.saturating_add(1);
                total_bytes = total_bytes.saturating_add(length);
            }
        }

        Ok(LogStats {
            file_count,
            total_bytes,
        })
    }

    pub fn snapshots_for_export(&self) -> Result<Vec<LogSnapshot>, DiagnosticsError> {
        let _guard = self.lock()?;
        self.prune_locked()?;

        let mut snapshots = Vec::new();
        let mut total_bytes = 0u64;
        for path in self.retained_log_paths() {
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                return Err(DiagnosticsError::UnsafeLogPath);
            };
            if safe_file_length(&path)?.is_none() {
                continue;
            }

            let mut content = Vec::new();
            open_private_read_file(&path)?
                .take(self.inner.config.max_file_bytes.saturating_add(1))
                .read_to_end(&mut content)
                .map_err(|source| DiagnosticsError::io("read log for export", source))?;
            if u64::try_from(content.len()).unwrap_or(u64::MAX) > self.inner.config.max_file_bytes {
                return Err(DiagnosticsError::LogSizeLimitExceeded);
            }

            // This is the second redaction pass. It protects old or externally
            // modified log files as well as records captured by this version.
            let redacted = self
                .inner
                .redactor
                .redact(&String::from_utf8_lossy(&content));
            self.inner.redactor.audit(&redacted)?;
            let content = redacted.into_bytes();
            total_bytes = total_bytes.saturating_add(
                u64::try_from(content.len())
                    .map_err(|_| DiagnosticsError::ExportSizeLimitExceeded)?,
            );
            if total_bytes > self.inner.config.max_export_bytes {
                return Err(DiagnosticsError::ExportSizeLimitExceeded);
            }

            snapshots.push(LogSnapshot {
                archive_name: format!("logs/{file_name}"),
                content,
            });
        }

        Ok(snapshots)
    }

    pub fn writer_factory(&self) -> DiagnosticsWriterFactory {
        DiagnosticsWriterFactory {
            store: self.clone(),
            buffer_limit: self.inner.config.max_record_bytes,
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, ()>, DiagnosticsError> {
        self.inner
            .lock
            .lock()
            .map_err(|_| DiagnosticsError::LogLockPoisoned)
    }

    fn prune_locked(&self) -> Result<(), DiagnosticsError> {
        for entry in fs::read_dir(&self.inner.directory)
            .map_err(|source| DiagnosticsError::io("list log directory", source))?
        {
            let entry = entry.map_err(|source| DiagnosticsError::io("read log entry", source))?;
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            let Some(index) = log_rotation_index(file_name) else {
                continue;
            };
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|source| DiagnosticsError::io("inspect log", source))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(DiagnosticsError::UnsafeLogPath);
            }

            let exceeds_file_count = index >= self.inner.config.max_files;
            let exceeds_file_size = metadata.len() > self.inner.config.max_file_bytes;
            if exceeds_file_count || exceeds_file_size {
                fs::remove_file(entry.path())
                    .map_err(|source| DiagnosticsError::io("prune log", source))?;
            } else {
                set_private_file_permissions(&entry.path())?;
            }
        }

        Ok(())
    }

    fn rotate_locked(&self) -> Result<(), DiagnosticsError> {
        if self.inner.config.max_files == 1 {
            remove_file_if_exists(&self.inner.directory.join(CURRENT_LOG_NAME))?;
            return Ok(());
        }

        let oldest = self
            .inner
            .directory
            .join(rotated_log_name(self.inner.config.max_files - 1));
        remove_file_if_exists(&oldest)?;

        for destination_index in (1..self.inner.config.max_files).rev() {
            let source = if destination_index == 1 {
                self.inner.directory.join(CURRENT_LOG_NAME)
            } else {
                self.inner
                    .directory
                    .join(rotated_log_name(destination_index - 1))
            };
            let destination = self
                .inner
                .directory
                .join(rotated_log_name(destination_index));

            if safe_file_length(&source)?.is_some() {
                remove_file_if_exists(&destination)?;
                fs::rename(&source, &destination)
                    .map_err(|source| DiagnosticsError::io("rotate log", source))?;
                set_private_file_permissions(&destination)?;
            }
        }

        Ok(())
    }

    fn retained_log_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::with_capacity(self.inner.config.max_files);
        paths.push(self.inner.directory.join(CURRENT_LOG_NAME));
        paths.extend(
            (1..self.inner.config.max_files)
                .map(|index| self.inner.directory.join(rotated_log_name(index))),
        );
        paths
    }
}

#[derive(Clone)]
pub struct DiagnosticsWriterFactory {
    store: LogStore,
    buffer_limit: usize,
}

impl<'writer> MakeWriter<'writer> for DiagnosticsWriterFactory {
    type Writer = DiagnosticsWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        DiagnosticsWriter {
            store: self.store.clone(),
            buffer: Vec::new(),
            buffer_limit: self.buffer_limit,
            discarded: false,
            flushed: false,
        }
    }
}

pub struct DiagnosticsWriter {
    store: LogStore,
    buffer: Vec<u8>,
    buffer_limit: usize,
    discarded: bool,
    flushed: bool,
}

impl DiagnosticsWriter {
    fn flush_record(&mut self) -> io::Result<()> {
        if self.flushed || (self.buffer.is_empty() && !self.discarded) {
            return Ok(());
        }

        if self.discarded {
            let available = self.buffer_limit.saturating_sub(self.buffer.len());
            let marker = TRUNCATION_MARKER.as_bytes();
            self.buffer
                .extend_from_slice(&marker[..marker.len().min(available)]);
        }

        self.store
            .append(&String::from_utf8_lossy(&self.buffer))
            .map_err(io::Error::other)?;
        self.flushed = true;
        Ok(())
    }
}

impl Write for DiagnosticsWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.flushed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "diagnostics writer has already been flushed",
            ));
        }

        let available = self.buffer_limit.saturating_sub(self.buffer.len());
        let retained = available.min(buffer.len());
        self.buffer.extend_from_slice(&buffer[..retained]);
        self.discarded |= retained < buffer.len();
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_record()
    }
}

impl Drop for DiagnosticsWriter {
    fn drop(&mut self) {
        let _ = self.flush_record();
    }
}

fn bounded_record(input: &str, max_bytes: usize) -> Vec<u8> {
    let has_newline = input.ends_with('\n');
    let newline_bytes = usize::from(!has_newline);
    let required = input.len().saturating_add(newline_bytes);
    if required <= max_bytes {
        let mut output = Vec::with_capacity(required);
        output.extend_from_slice(input.as_bytes());
        if !has_newline {
            output.push(b'\n');
        }
        return output;
    }

    let marker_bytes = TRUNCATION_MARKER.as_bytes();
    let content_limit = max_bytes.saturating_sub(marker_bytes.len().saturating_add(1));
    let mut boundary = content_limit.min(input.len());
    while boundary > 0 && !input.is_char_boundary(boundary) {
        boundary -= 1;
    }

    let mut output = Vec::with_capacity(max_bytes);
    output.extend_from_slice(&input.as_bytes()[..boundary]);
    let marker_length = marker_bytes
        .len()
        .min(max_bytes.saturating_sub(output.len() + 1));
    output.extend_from_slice(&marker_bytes[..marker_length]);
    if output.len() < max_bytes {
        output.push(b'\n');
    }
    output
}

fn ensure_directory(
    path: &Path,
    enforce_private_permissions: bool,
) -> Result<(), DiagnosticsError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(DiagnosticsError::UnsafeLogPath);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_private_directory(path)?;
        }
        Err(source) => return Err(DiagnosticsError::io("inspect log directory", source)),
    }

    if enforce_private_permissions {
        set_private_directory_permissions(path)?;
    }
    Ok(())
}

fn safe_file_length(path: &Path) -> Result<Option<u64>, DiagnosticsError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(DiagnosticsError::UnsafeLogPath);
            }
            Ok(Some(metadata.len()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(DiagnosticsError::io("inspect log", source)),
    }
}

fn remove_file_if_exists(path: &Path) -> Result<(), DiagnosticsError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(DiagnosticsError::UnsafeLogPath);
            }
            fs::remove_file(path).map_err(|source| DiagnosticsError::io("remove log", source))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(DiagnosticsError::io("inspect log", source)),
    }
}

fn log_rotation_index(file_name: &str) -> Option<usize> {
    if file_name == CURRENT_LOG_NAME {
        return Some(0);
    }
    let index = file_name
        .strip_prefix("app.")?
        .strip_suffix(".log")?
        .parse::<usize>()
        .ok()?;
    (index > 0).then_some(index)
}

fn rotated_log_name(index: usize) -> String {
    format!("app.{index}.log")
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> Result<(), DiagnosticsError> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder
        .create(path)
        .map_err(|source| DiagnosticsError::io("create log directory", source))
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> Result<(), DiagnosticsError> {
    fs::create_dir_all(path).map_err(|source| DiagnosticsError::io("create log directory", source))
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), DiagnosticsError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|source| DiagnosticsError::io("secure log directory", source))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), DiagnosticsError> {
    Ok(())
}

#[cfg(unix)]
fn open_private_append_file(path: &Path) -> Result<File, DiagnosticsError> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|source| DiagnosticsError::io("open log", source))?;
    set_private_file_permissions(path)?;
    Ok(file)
}

#[cfg(windows)]
fn open_private_append_file(path: &Path) -> Result<File, DiagnosticsError> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|source| DiagnosticsError::io("open log", source))
}

#[cfg(not(any(unix, windows)))]
fn open_private_append_file(path: &Path) -> Result<File, DiagnosticsError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| DiagnosticsError::io("open log", source))
}

#[cfg(unix)]
fn open_private_read_file(path: &Path) -> Result<File, DiagnosticsError> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|source| DiagnosticsError::io("open log for export", source))
}

#[cfg(windows)]
fn open_private_read_file(path: &Path) -> Result<File, DiagnosticsError> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|source| DiagnosticsError::io("open log for export", source))
}

#[cfg(not(any(unix, windows)))]
fn open_private_read_file(path: &Path) -> Result<File, DiagnosticsError> {
    File::open(path).map_err(|source| DiagnosticsError::io("open log for export", source))
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), DiagnosticsError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|source| DiagnosticsError::io("secure log", source))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), DiagnosticsError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn test_config() -> DiagnosticsConfig {
        DiagnosticsConfig {
            max_file_bytes: 160,
            max_files: 3,
            max_record_bytes: 100,
            max_export_bytes: 480 + crate::REPORT_EXPORT_BUDGET_BYTES,
        }
    }

    fn open_store(temp: &TempDir) -> LogStore {
        LogStore::open(
            temp.path(),
            test_config(),
            Redactor::new().expect("redactor should initialize"),
        )
        .expect("log store should open")
    }

    #[test]
    fn redacts_before_writing_to_disk() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let store = open_store(&temp);

        store
            .append("request token=super-secret at https://example.com/private?key=value")
            .expect("log should be written");

        let content =
            fs::read_to_string(temp.path().join("logs/app.log")).expect("log should be readable");
        assert!(!content.contains("super-secret"));
        assert!(!content.contains("example.com"));
        assert!(content.contains("[REDACTED_"));
    }

    #[test]
    fn rotation_enforces_file_count_and_size_limits() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let store = open_store(&temp);

        for index in 0..12 {
            store
                .append(&format!(
                    "event {index}: repeated safe words repeated safe words repeated safe words"
                ))
                .expect("log should be written");
        }

        let stats = store.stats().expect("stats should load");
        assert!(stats.file_count <= 3);
        assert!(stats.total_bytes <= 480);
        for path in store.retained_log_paths() {
            if let Some(length) = safe_file_length(&path).expect("metadata should load") {
                assert!(length <= 160);
            }
        }
    }

    #[test]
    fn second_pass_redacts_externally_modified_log() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let store = open_store(&temp);
        fs::write(
            temp.path().join("logs/app.log"),
            "legacy raw https://user:pass@proxy.example.com/path?token=hidden\n",
        )
        .expect("legacy log should be written");

        let snapshots = store
            .snapshots_for_export()
            .expect("snapshot should be safe");
        let content = String::from_utf8_lossy(&snapshots[0].content);
        assert!(!content.contains("user:pass"));
        assert!(!content.contains("proxy.example.com"));
        assert!(!content.contains("hidden"));
    }

    #[test]
    fn oversized_existing_logs_are_pruned() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let store = open_store(&temp);
        let log_path = temp.path().join("logs/app.log");
        fs::write(&log_path, vec![b'x'; 161]).expect("oversized log should be written");

        store.prune().expect("pruning should succeed");

        assert!(!log_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn log_directory_and_files_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().expect("temporary directory should be created");
        let store = open_store(&temp);
        store.append("safe event").expect("log should be written");

        let directory_mode = fs::metadata(temp.path().join("logs"))
            .expect("directory metadata should load")
            .permissions()
            .mode()
            & 0o777;
        let file_mode = fs::metadata(temp.path().join("logs/app.log"))
            .expect("file metadata should load")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(directory_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_log_files() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("temporary directory should be created");
        let outside = temp.path().join("outside.log");
        fs::write(&outside, "private").expect("outside file should be written");
        fs::create_dir(temp.path().join("logs")).expect("log directory should be created");
        symlink(&outside, temp.path().join("logs/app.log")).expect("symlink should be created");

        let error = LogStore::open(
            temp.path(),
            test_config(),
            Redactor::new().expect("redactor should initialize"),
        )
        .err()
        .expect("symlink should be rejected");

        assert!(matches!(error, DiagnosticsError::UnsafeLogPath));
    }
}
