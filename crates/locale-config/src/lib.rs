use serde_json::{Map, Value as JsonValue};
use shared_types::LocaleStatus;
use std::env;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use toml_edit::{value, DocumentMut, Item, Table};

const CHINESE_LOCALE: &str = "zh-CN";
const BACKUP_SUFFIX: &str = ".wocao-hub.bak";
const MISSING_SUFFIX: &str = ".wocao-hub.missing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalePaths {
    pub config_path: PathBuf,
    pub global_state_path: PathBuf,
}

impl LocalePaths {
    #[must_use]
    pub fn from_codex_home(codex_home: impl AsRef<Path>) -> Self {
        let codex_home = codex_home.as_ref();
        Self {
            config_path: codex_home.join("config.toml"),
            global_state_path: codex_home.join(".codex-global-state.json"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleApplyOutcome {
    pub status: LocaleStatus,
    pub config_changed: bool,
    pub global_state_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleRestoreOutcome {
    pub status: LocaleStatus,
    pub restored_files: Vec<String>,
}

#[derive(Debug, Error)]
pub enum LocaleConfigError {
    #[error("无法确定当前用户目录")]
    HomeDirectoryNotFound,
    #[error("读取配置文件失败（{path}）：{source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("写入配置文件失败（{path}）：{source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("配置文件不是有效的 UTF-8 文本（{0}）")]
    InvalidUtf8(String),
    #[error("Codex TOML 配置无效：{0}")]
    InvalidToml(#[from] toml_edit::TomlError),
    #[error("[desktop] 配置格式无效，无法安全写入中文设置")]
    InvalidDesktopSection,
    #[error("Codex 全局状态 JSON 无效：{0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("Codex 全局状态必须是 JSON 对象")]
    InvalidGlobalState,
    #[error("没有可恢复的原始中文配置备份")]
    BackupNotFound,
}

pub fn default_locale_paths() -> Result<LocalePaths, LocaleConfigError> {
    if let Some(codex_home) = env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        return Ok(LocalePaths::from_codex_home(PathBuf::from(codex_home)));
    }
    let home = dirs::home_dir().ok_or(LocaleConfigError::HomeDirectoryNotFound)?;
    Ok(LocalePaths::from_codex_home(home.join(".codex")))
}

pub fn inspect_locale(paths: &LocalePaths) -> Result<LocaleStatus, LocaleConfigError> {
    let config_locale = match read_optional(&paths.config_path)? {
        Some(raw) => Some(read_config_locale(&decode(&paths.config_path, raw)?)?).flatten(),
        None => None,
    };
    let global_state_locale = match read_optional(&paths.global_state_path)? {
        Some(raw) => Some(read_global_state_locale(&decode(
            &paths.global_state_path,
            raw,
        )?)?)
        .flatten(),
        None => None,
    };
    let config_is_chinese = config_locale.as_deref() == Some(CHINESE_LOCALE);
    let state_is_compatible = global_state_locale
        .as_deref()
        .is_none_or(|locale| locale == CHINESE_LOCALE);

    Ok(LocaleStatus {
        chinese_enabled: config_is_chinese && state_is_compatible,
        config_locale,
        global_state_locale,
        config_path: paths.config_path.to_string_lossy().into_owned(),
        global_state_path: paths.global_state_path.to_string_lossy().into_owned(),
        restore_available: has_restore_data(&paths.config_path)
            || has_restore_data(&paths.global_state_path),
    })
}

pub fn apply_chinese_locale(paths: &LocalePaths) -> Result<LocaleApplyOutcome, LocaleConfigError> {
    let original_config = read_optional(&paths.config_path)?;
    let original_state = read_optional(&paths.global_state_path)?;
    let config_text = original_config
        .as_ref()
        .map(|raw| decode(&paths.config_path, raw.clone()))
        .transpose()?
        .unwrap_or_default();
    let state_text = original_state
        .as_ref()
        .map(|raw| decode(&paths.global_state_path, raw.clone()))
        .transpose()?
        .unwrap_or_default();

    let updated_config = set_chinese_locale(&config_text)?;
    let updated_state = set_global_state_chinese_locale(&state_text)?;
    let config_changed = updated_config.as_bytes() != config_text.as_bytes();
    let global_state_changed = updated_state.as_bytes() != state_text.as_bytes();

    if config_changed {
        prepare_backup(&paths.config_path, original_config.as_deref())?;
    }
    if global_state_changed {
        prepare_backup(&paths.global_state_path, original_state.as_deref())?;
    }

    if let Err(error) = write_updates(
        paths,
        config_changed.then_some(updated_config.as_bytes()),
        global_state_changed.then_some(updated_state.as_bytes()),
    ) {
        let _ = restore_snapshot(&paths.config_path, original_config.as_deref());
        let _ = restore_snapshot(&paths.global_state_path, original_state.as_deref());
        return Err(error);
    }

    Ok(LocaleApplyOutcome {
        status: inspect_locale(paths)?,
        config_changed,
        global_state_changed,
    })
}

pub fn restore_locale(paths: &LocalePaths) -> Result<LocaleRestoreOutcome, LocaleConfigError> {
    let mut restored_files = Vec::new();
    if restore_backup(&paths.config_path)? {
        restored_files.push(paths.config_path.to_string_lossy().into_owned());
    }
    if restore_backup(&paths.global_state_path)? {
        restored_files.push(paths.global_state_path.to_string_lossy().into_owned());
    }
    if restored_files.is_empty() {
        return Err(LocaleConfigError::BackupNotFound);
    }

    Ok(LocaleRestoreOutcome {
        status: inspect_locale(paths)?,
        restored_files,
    })
}

pub fn set_chinese_locale(input: &str) -> Result<String, LocaleConfigError> {
    let mut document = if input.trim().is_empty() {
        DocumentMut::new()
    } else {
        input.parse::<DocumentMut>()?
    };
    if !document.contains_key("desktop") {
        document["desktop"] = Item::Table(Table::new());
    }
    let desktop = document
        .get_mut("desktop")
        .and_then(Item::as_table_mut)
        .ok_or(LocaleConfigError::InvalidDesktopSection)?;
    desktop["localeOverride"] = value(CHINESE_LOCALE);
    Ok(document.to_string())
}

fn read_config_locale(input: &str) -> Result<Option<String>, LocaleConfigError> {
    if input.trim().is_empty() {
        return Ok(None);
    }
    let document = input.parse::<DocumentMut>()?;
    let Some(desktop) = document.get("desktop") else {
        return Ok(None);
    };
    let desktop = desktop
        .as_table()
        .ok_or(LocaleConfigError::InvalidDesktopSection)?;
    Ok(desktop
        .get("localeOverride")
        .and_then(Item::as_value)
        .and_then(toml_edit::Value::as_str)
        .map(str::to_owned))
}

fn set_global_state_chinese_locale(input: &str) -> Result<String, LocaleConfigError> {
    let mut state = if input.trim().is_empty() {
        JsonValue::Object(Map::new())
    } else {
        serde_json::from_str(input)?
    };
    let object = state
        .as_object_mut()
        .ok_or(LocaleConfigError::InvalidGlobalState)?;
    if object.get("localeOverride").and_then(JsonValue::as_str) == Some(CHINESE_LOCALE) {
        return Ok(input.to_owned());
    }
    object.insert(
        "localeOverride".to_owned(),
        JsonValue::String(CHINESE_LOCALE.to_owned()),
    );
    let is_pretty = input.trim_end().contains('\n');
    let mut output = if is_pretty {
        serde_json::to_string_pretty(&state)?
    } else {
        serde_json::to_string(&state)?
    };
    output.push('\n');
    Ok(output)
}

fn read_global_state_locale(input: &str) -> Result<Option<String>, LocaleConfigError> {
    if input.trim().is_empty() {
        return Ok(None);
    }
    let state: JsonValue = serde_json::from_str(input)?;
    let object = state
        .as_object()
        .ok_or(LocaleConfigError::InvalidGlobalState)?;
    Ok(object
        .get("localeOverride")
        .and_then(JsonValue::as_str)
        .map(str::to_owned))
}

fn write_updates(
    paths: &LocalePaths,
    config: Option<&[u8]>,
    global_state: Option<&[u8]>,
) -> Result<(), LocaleConfigError> {
    if let Some(config) = config {
        atomic_write(&paths.config_path, config)?;
    }
    if let Some(global_state) = global_state {
        atomic_write(&paths.global_state_path, global_state)?;
    }
    Ok(())
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, LocaleConfigError> {
    match fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(LocaleConfigError::Read {
            path: path.to_string_lossy().into_owned(),
            source,
        }),
    }
}

fn decode(path: &Path, raw: Vec<u8>) -> Result<String, LocaleConfigError> {
    String::from_utf8(raw)
        .map_err(|_| LocaleConfigError::InvalidUtf8(path.to_string_lossy().into_owned()))
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), LocaleConfigError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| LocaleConfigError::Write {
        path: parent.to_string_lossy().into_owned(),
        source,
    })?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut temp_name = path
        .file_name()
        .map_or_else(|| OsString::from("config"), OsString::from);
    temp_name.push(format!(".wocao-hub.tmp-{}-{nonce}", std::process::id()));
    let temp_path = parent.join(temp_name);

    let write_result = (|| -> Result<(), std::io::Error> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
        set_safe_permissions(path, &temp_path)?;

        #[cfg(target_os = "windows")]
        if path.exists() {
            fs::remove_file(path)?;
        }
        fs::rename(&temp_path, path)?;
        Ok(())
    })();

    if let Err(source) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(LocaleConfigError::Write {
            path: path.to_string_lossy().into_owned(),
            source,
        });
    }
    Ok(())
}

#[cfg(unix)]
fn set_safe_permissions(original: &Path, temporary: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(original)
        .map(|metadata| metadata.permissions().mode())
        .unwrap_or(0o600);
    fs::set_permissions(temporary, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_safe_permissions(_original: &Path, _temporary: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

fn prepare_backup(path: &Path, original: Option<&[u8]>) -> Result<(), LocaleConfigError> {
    if has_restore_data(path) {
        return Ok(());
    }
    match original {
        Some(contents) => atomic_write(&suffixed(path, BACKUP_SUFFIX), contents),
        None => atomic_write(&suffixed(path, MISSING_SUFFIX), b"missing\n"),
    }
}

fn restore_backup(path: &Path) -> Result<bool, LocaleConfigError> {
    let backup = suffixed(path, BACKUP_SUFFIX);
    let missing = suffixed(path, MISSING_SUFFIX);
    if let Some(contents) = read_optional(&backup)? {
        atomic_write(path, &contents)?;
        remove_restore_file(&backup)?;
        let _ = fs::remove_file(&missing);
        return Ok(true);
    }
    if missing.exists() {
        if path.exists() {
            fs::remove_file(path).map_err(|source| LocaleConfigError::Write {
                path: path.to_string_lossy().into_owned(),
                source,
            })?;
        }
        remove_restore_file(&missing)?;
        return Ok(true);
    }
    Ok(false)
}

fn restore_snapshot(path: &Path, original: Option<&[u8]>) -> Result<(), LocaleConfigError> {
    match original {
        Some(contents) => atomic_write(path, contents),
        None => {
            if path.exists() {
                fs::remove_file(path).map_err(|source| LocaleConfigError::Write {
                    path: path.to_string_lossy().into_owned(),
                    source,
                })?;
            }
            Ok(())
        }
    }
}

fn remove_restore_file(path: &Path) -> Result<(), LocaleConfigError> {
    fs::remove_file(path).map_err(|source| LocaleConfigError::Write {
        path: path.to_string_lossy().into_owned(),
        source,
    })
}

fn has_restore_data(path: &Path) -> bool {
    suffixed(path, BACKUP_SUFFIX).is_file() || suffixed(path, MISSING_SUFFIX).is_file()
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_locale_override_without_losing_existing_config() {
        let output = set_chinese_locale("model = \"gpt-5\"\n").expect("valid TOML");
        assert!(output.contains("model = \"gpt-5\""));
        assert!(output.contains("[desktop]"));
        assert!(output.contains("localeOverride = \"zh-CN\""));
    }

    #[test]
    fn replaces_locale_and_is_idempotent() {
        let input = "[desktop]\nlocaleOverride = \"en-US\"\n\n[features]\njs_repl = false\n";
        let first = set_chinese_locale(input).expect("valid TOML");
        let second = set_chinese_locale(&first).expect("valid TOML");
        assert_eq!(first, second);
        assert_eq!(first.matches("localeOverride").count(), 1);
    }

    #[test]
    fn rejects_non_table_desktop_config() {
        let error = set_chinese_locale("desktop = true\n").expect_err("invalid desktop section");
        assert!(matches!(error, LocaleConfigError::InvalidDesktopSection));
    }

    #[test]
    fn applies_and_restores_existing_files() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = LocalePaths::from_codex_home(temp.path().join(".codex"));
        fs::create_dir_all(paths.config_path.parent().expect("config parent"))
            .expect("create config directory");
        let original_config = b"model = \"gpt-5\"\n";
        let original_state = br#"{"theme":"dark"}
"#;
        fs::write(&paths.config_path, original_config).expect("write config");
        fs::write(&paths.global_state_path, original_state).expect("write state");

        let applied = apply_chinese_locale(&paths).expect("apply locale");
        assert!(applied.config_changed);
        assert!(applied.global_state_changed);
        assert!(applied.status.chinese_enabled);
        assert!(applied.status.restore_available);

        let restored = restore_locale(&paths).expect("restore locale");
        assert_eq!(restored.restored_files.len(), 2);
        assert_eq!(
            fs::read(&paths.config_path).expect("read config"),
            original_config
        );
        assert_eq!(
            fs::read(&paths.global_state_path).expect("read state"),
            original_state
        );
        assert!(!restored.status.restore_available);
    }

    #[test]
    fn restore_removes_files_that_did_not_exist_before_apply() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = LocalePaths::from_codex_home(temp.path().join(".codex"));

        let applied = apply_chinese_locale(&paths).expect("apply locale");
        assert!(applied.status.chinese_enabled);
        assert!(paths.config_path.exists());
        assert!(paths.global_state_path.exists());

        restore_locale(&paths).expect("restore locale");
        assert!(!paths.config_path.exists());
        assert!(!paths.global_state_path.exists());
    }

    #[test]
    fn refuses_to_overwrite_invalid_json() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = LocalePaths::from_codex_home(temp.path().join(".codex"));
        fs::create_dir_all(paths.config_path.parent().expect("config parent"))
            .expect("create config directory");
        fs::write(&paths.global_state_path, b"not-json").expect("write invalid state");

        let error = apply_chinese_locale(&paths).expect_err("invalid JSON must fail");
        assert!(matches!(error, LocaleConfigError::InvalidJson(_)));
        assert_eq!(
            fs::read(&paths.global_state_path).expect("read invalid state"),
            b"not-json"
        );
    }

    #[test]
    fn keeps_compact_global_state_key_order() {
        let input = "{\"z\":1,\"a\":{\"enabled\":true}}\n";
        let output = set_global_state_chinese_locale(input).expect("valid global state");
        assert_eq!(
            output,
            "{\"z\":1,\"a\":{\"enabled\":true},\"localeOverride\":\"zh-CN\"}\n"
        );
        assert_eq!(
            set_global_state_chinese_locale(&output).expect("idempotent global state"),
            output
        );
    }
}
