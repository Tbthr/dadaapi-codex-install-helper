pub mod catalog;

use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use futures_util::StreamExt;
use reqwest::{
    header::{HeaderValue, CONTENT_LENGTH, CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE},
    Client, Response, StatusCode,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Notify,
};
use url::Url;

const HASH_BUFFER_SIZE: usize = 64 * 1024;
const RESUME_METADATA_MAGIC: &[u8; 8] = b"WHDPART1";
const MAX_VALIDATOR_LENGTH: usize = 8 * 1024;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("download request failed")]
    Request {
        #[source]
        source: reqwest::Error,
    },
    #[error("failed while reading the download response")]
    ResponseBody {
        #[source]
        source: reqwest::Error,
    },
    #[error("download file operation failed")]
    Io(#[from] io::Error),
    #[error("download was cancelled")]
    Cancelled,
    #[error("target path must identify a file")]
    InvalidTargetPath,
    #[error("target file already exists")]
    TargetAlreadyExists,
    #[error("partial download path is not a regular file")]
    InvalidPartialFile,
    #[error("server returned unsupported HTTP status {0}")]
    UnexpectedStatus(StatusCode),
    #[error("server returned an invalid Content-Length header")]
    InvalidContentLength,
    #[error("server returned an invalid Content-Range header")]
    InvalidContentRange,
    #[error("range response starts at byte {actual}, expected byte {expected}")]
    RangeStartMismatch { expected: u64, actual: u64 },
    #[error("partial file has {partial} bytes but the remote file has {remote} bytes")]
    PartialExceedsRemote { partial: u64, remote: u64 },
    #[error("partial file has {partial} bytes but the expected file has {expected} bytes")]
    PartialExceedsExpected { partial: u64, expected: u64 },
    #[error("server reports {actual} bytes but {expected} bytes were expected")]
    ExpectedLengthMismatch { expected: u64, actual: u64 },
    #[error("response body has {actual} bytes but {expected} bytes were declared")]
    ResponseLengthMismatch { expected: u64, actual: u64 },
    #[error("downloaded file has {actual} bytes but {expected} bytes were expected")]
    FinalLengthMismatch { expected: u64, actual: u64 },
    #[error("download size exceeds the supported range")]
    LengthOverflow,
    #[error("downloaded file checksum does not match the expected SHA-256")]
    ChecksumMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    #[error("range response validator does not match the partial download")]
    ResumeValidatorMismatch,
}

#[derive(Debug, Default)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

/// A clonable cancellation handle for an in-flight download.
#[derive(Debug, Clone, Default)]
pub struct DownloadCancellation {
    state: Arc<CancellationState>,
}

impl DownloadCancellation {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        if !self.state.cancelled.swap(true, Ordering::AcqRel) {
            self.state.notify.notify_waiters();
        }
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    async fn cancelled(&self) {
        let notified = self.state.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.is_cancelled() {
            return;
        }

        notified.await;
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    url: Url,
    target_path: PathBuf,
    expected_length: Option<u64>,
    expected_sha256: Option<[u8; 32]>,
    cancellation: DownloadCancellation,
}

impl DownloadRequest {
    #[must_use]
    pub fn new(url: Url, target_path: impl Into<PathBuf>) -> Self {
        Self {
            url,
            target_path: target_path.into(),
            expected_length: None,
            expected_sha256: None,
            cancellation: DownloadCancellation::new(),
        }
    }

    #[must_use]
    pub fn with_expected_length(mut self, expected_length: u64) -> Self {
        self.expected_length = Some(expected_length);
        self
    }

    #[must_use]
    pub fn with_expected_sha256(mut self, expected_sha256: [u8; 32]) -> Self {
        self.expected_sha256 = Some(expected_sha256);
        self
    }

    #[must_use]
    pub fn with_cancellation(mut self, cancellation: DownloadCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }

    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }

    #[must_use]
    pub fn target_path(&self) -> &Path {
        &self.target_path
    }

    #[must_use]
    pub fn part_path(&self) -> PathBuf {
        partial_path_for(&self.target_path)
    }

    #[must_use]
    pub fn metadata_path(&self) -> PathBuf {
        resume_metadata_path_for(&self.target_path)
    }

    #[must_use]
    pub fn cancellation(&self) -> &DownloadCancellation {
        &self.cancellation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    /// Bytes currently stored in the partial file, including resumed bytes.
    pub downloaded_bytes: u64,
    /// Complete remote size when supplied by the server or caller.
    pub total_bytes: Option<u64>,
    /// Bytes reused from the partial file for this request.
    pub resumed_from: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    pub target_path: PathBuf,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub resumed_from: u64,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct DownloadClient {
    client: Client,
}

impl Default for DownloadClient {
    fn default() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl DownloadClient {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn content_length(&self, url: Url) -> Result<Option<u64>, DownloadError> {
        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(|source| DownloadError::Request { source })?;

        if !response.status().is_success() {
            return Err(DownloadError::UnexpectedStatus(response.status()));
        }

        response_content_length(&response)
    }

    pub async fn download(
        &self,
        request: DownloadRequest,
    ) -> Result<DownloadOutcome, DownloadError> {
        self.download_with_progress(request, |_| {}).await
    }

    pub async fn download_with_progress<F>(
        &self,
        request: DownloadRequest,
        mut on_progress: F,
    ) -> Result<DownloadOutcome, DownloadError>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        validate_target_path(&request.target_path)?;
        ensure_target_absent(&request.target_path).await?;
        create_target_parent(&request.target_path).await?;

        let part_path = request.part_path();
        let metadata_path = request.metadata_path();
        let partial_length = partial_file_length(&part_path).await?;
        if let Some(expected) = request.expected_length {
            if partial_length > expected {
                return Err(DownloadError::PartialExceedsExpected {
                    partial: partial_length,
                    expected,
                });
            }
        }

        ensure_not_cancelled(&request.cancellation)?;

        if partial_length > 0
            && request.expected_length == Some(partial_length)
            && request.expected_sha256.is_some()
        {
            let sha256 = hash_file(&part_path, &request.cancellation).await?;
            if verify_checksum(request.expected_sha256, sha256).is_ok() {
                on_progress(DownloadProgress {
                    downloaded_bytes: partial_length,
                    total_bytes: Some(partial_length),
                    resumed_from: partial_length,
                });
                ensure_not_cancelled(&request.cancellation)?;
                finalize_download(&part_path, &metadata_path, &request.target_path).await?;
                return Ok(DownloadOutcome {
                    target_path: request.target_path,
                    total_bytes: partial_length,
                    transferred_bytes: 0,
                    resumed_from: partial_length,
                    sha256,
                });
            }

            remove_file_if_exists(&metadata_path).await?;
        }

        let resume_validator = if partial_length > 0 {
            load_resume_validator(&metadata_path, &request.url).await?
        } else {
            None
        };
        let requested_offset = resume_validator.as_ref().map_or(0, |_| partial_length);

        let mut builder = self.client.get(request.url.clone());
        if let Some(validator) = &resume_validator {
            builder = builder
                .header(RANGE, format!("bytes={requested_offset}-"))
                .header(IF_RANGE, validator.header_value().clone());
        }

        let response = tokio::select! {
            biased;
            () = request.cancellation.cancelled() => return Err(DownloadError::Cancelled),
            result = builder.send() => result.map_err(|source| DownloadError::Request { source })?,
        };

        let response_plan = plan_response(
            &response,
            requested_offset,
            request.expected_length,
            resume_validator.as_ref(),
        )?;

        if response_plan.completed_partial {
            on_progress(DownloadProgress {
                downloaded_bytes: partial_length,
                total_bytes: response_plan.total_length,
                resumed_from: partial_length,
            });
            ensure_not_cancelled(&request.cancellation)?;
            let sha256 = hash_file(&part_path, &request.cancellation).await?;
            if let Err(error) = verify_checksum(request.expected_sha256, sha256) {
                remove_file_if_exists(&metadata_path).await?;
                return Err(error);
            }
            ensure_not_cancelled(&request.cancellation)?;
            finalize_download(&part_path, &metadata_path, &request.target_path).await?;
            return Ok(DownloadOutcome {
                target_path: request.target_path,
                total_bytes: partial_length,
                transferred_bytes: 0,
                resumed_from: partial_length,
                sha256,
            });
        }

        let mut file = open_partial_file(&part_path, response_plan.append).await?;
        if !response_plan.append {
            file.sync_all().await?;
            replace_resume_metadata(
                &metadata_path,
                &request.url,
                response_plan.validator.as_ref(),
            )
            .await?;
        }
        let resumed_from = response_plan.write_offset;
        let mut transferred_bytes = 0_u64;

        on_progress(DownloadProgress {
            downloaded_bytes: resumed_from,
            total_bytes: response_plan.total_length,
            resumed_from,
        });
        ensure_not_cancelled(&request.cancellation)?;

        let mut body = response.bytes_stream();
        loop {
            let next = tokio::select! {
                biased;
                () = request.cancellation.cancelled() => return Err(DownloadError::Cancelled),
                item = body.next() => item,
            };

            let Some(chunk) = next else {
                break;
            };
            let chunk = chunk.map_err(|source| DownloadError::ResponseBody { source })?;
            let chunk_length =
                u64::try_from(chunk.len()).map_err(|_| DownloadError::LengthOverflow)?;
            let next_transferred = transferred_bytes
                .checked_add(chunk_length)
                .ok_or(DownloadError::LengthOverflow)?;

            if let Some(expected) = response_plan.response_length {
                if next_transferred > expected {
                    return Err(DownloadError::ResponseLengthMismatch {
                        expected,
                        actual: next_transferred,
                    });
                }
            }

            file.write_all(&chunk).await?;
            transferred_bytes = next_transferred;
            let downloaded_bytes = resumed_from
                .checked_add(transferred_bytes)
                .ok_or(DownloadError::LengthOverflow)?;

            on_progress(DownloadProgress {
                downloaded_bytes,
                total_bytes: response_plan.total_length,
                resumed_from,
            });
            ensure_not_cancelled(&request.cancellation)?;
        }

        if let Some(expected) = response_plan.response_length {
            if transferred_bytes != expected {
                return Err(DownloadError::ResponseLengthMismatch {
                    expected,
                    actual: transferred_bytes,
                });
            }
        }

        let final_length = resumed_from
            .checked_add(transferred_bytes)
            .ok_or(DownloadError::LengthOverflow)?;
        if let Some(expected) = response_plan.total_length {
            if final_length != expected {
                return Err(DownloadError::FinalLengthMismatch {
                    expected,
                    actual: final_length,
                });
            }
        }

        file.flush().await?;
        file.sync_all().await?;
        let actual_file_length = file.metadata().await?.len();
        if actual_file_length != final_length {
            return Err(DownloadError::FinalLengthMismatch {
                expected: final_length,
                actual: actual_file_length,
            });
        }
        drop(file);

        let sha256 = hash_file(&part_path, &request.cancellation).await?;
        if let Err(error) = verify_checksum(request.expected_sha256, sha256) {
            remove_file_if_exists(&metadata_path).await?;
            return Err(error);
        }
        ensure_not_cancelled(&request.cancellation)?;
        finalize_download(&part_path, &metadata_path, &request.target_path).await?;

        Ok(DownloadOutcome {
            target_path: request.target_path,
            total_bytes: final_length,
            transferred_bytes,
            resumed_from,
            sha256,
        })
    }
}

#[must_use]
pub fn partial_path_for(target_path: &Path) -> PathBuf {
    let mut path = OsString::from(target_path.as_os_str());
    path.push(".part");
    PathBuf::from(path)
}

/// Returns the sidecar path used to bind a partial file to its URL and validator.
#[must_use]
pub fn resume_metadata_path_for(target_path: &Path) -> PathBuf {
    let mut path = OsString::from(target_path.as_os_str());
    path.push(".part.meta");
    PathBuf::from(path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResumeValidator {
    StrongEtag(HeaderValue),
    LastModified(HeaderValue),
}

impl ResumeValidator {
    fn header_value(&self) -> &HeaderValue {
        match self {
            Self::StrongEtag(value) | Self::LastModified(value) => value,
        }
    }

    fn kind(&self) -> u8 {
        match self {
            Self::StrongEtag(_) => 1,
            Self::LastModified(_) => 2,
        }
    }
}

#[derive(Debug, Clone)]
struct ResponsePlan {
    append: bool,
    completed_partial: bool,
    write_offset: u64,
    response_length: Option<u64>,
    total_length: Option<u64>,
    validator: Option<ResumeValidator>,
}

fn plan_response(
    response: &Response,
    partial_length: u64,
    expected_length: Option<u64>,
    resume_validator: Option<&ResumeValidator>,
) -> Result<ResponsePlan, DownloadError> {
    let status = response.status();

    if partial_length == 0 {
        if status != StatusCode::OK {
            return Err(DownloadError::UnexpectedStatus(status));
        }

        let response_length = response_content_length(response)?;
        validate_reported_total(response_length, expected_length)?;
        return Ok(ResponsePlan {
            append: false,
            completed_partial: false,
            write_offset: 0,
            response_length: response_length.or(expected_length),
            total_length: response_length.or(expected_length),
            validator: preferred_response_validator(response),
        });
    }

    match status {
        StatusCode::PARTIAL_CONTENT => {
            let resume_validator =
                resume_validator.ok_or(DownloadError::ResumeValidatorMismatch)?;
            validate_response_validator(response, resume_validator)?;
            let range = parse_satisfied_content_range(response)?;
            if range.start != partial_length {
                return Err(DownloadError::RangeStartMismatch {
                    expected: partial_length,
                    actual: range.start,
                });
            }

            if let Some(total) = range.total {
                if partial_length >= total {
                    return Err(DownloadError::PartialExceedsRemote {
                        partial: partial_length,
                        remote: total,
                    });
                }
            }

            validate_reported_total(range.total, expected_length)?;
            let total_length = range
                .total
                .or(expected_length)
                .ok_or(DownloadError::InvalidContentRange)?;
            let range_length = range
                .end
                .checked_sub(range.start)
                .and_then(|length| length.checked_add(1))
                .ok_or(DownloadError::InvalidContentRange)?;
            let header_length = response_content_length(response)?;
            if let Some(actual) = header_length {
                if actual != range_length {
                    return Err(DownloadError::ResponseLengthMismatch {
                        expected: range_length,
                        actual,
                    });
                }
            }

            Ok(ResponsePlan {
                append: true,
                completed_partial: false,
                write_offset: partial_length,
                response_length: Some(range_length),
                total_length: Some(total_length),
                validator: Some(resume_validator.clone()),
            })
        }
        StatusCode::OK => {
            let response_length = response_content_length(response)?;
            validate_reported_total(response_length, expected_length)?;
            Ok(ResponsePlan {
                append: false,
                completed_partial: false,
                write_offset: 0,
                response_length: response_length.or(expected_length),
                total_length: response_length.or(expected_length),
                validator: preferred_response_validator(response),
            })
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            let resume_validator =
                resume_validator.ok_or(DownloadError::ResumeValidatorMismatch)?;
            validate_response_validator(response, resume_validator)?;
            let remote_length = parse_unsatisfied_content_range(response)?;
            validate_reported_total(Some(remote_length), expected_length)?;
            if partial_length > remote_length {
                return Err(DownloadError::PartialExceedsRemote {
                    partial: partial_length,
                    remote: remote_length,
                });
            }
            if partial_length != remote_length {
                return Err(DownloadError::UnexpectedStatus(status));
            }

            Ok(ResponsePlan {
                append: true,
                completed_partial: true,
                write_offset: partial_length,
                response_length: Some(0),
                total_length: Some(remote_length),
                validator: Some(resume_validator.clone()),
            })
        }
        _ => Err(DownloadError::UnexpectedStatus(status)),
    }
}

#[derive(Debug, Clone, Copy)]
struct SatisfiedContentRange {
    start: u64,
    end: u64,
    total: Option<u64>,
}

fn preferred_response_validator(response: &Response) -> Option<ResumeValidator> {
    response
        .headers()
        .get(ETAG)
        .filter(|value| is_strong_etag(value))
        .cloned()
        .map(ResumeValidator::StrongEtag)
        .or_else(|| {
            response
                .headers()
                .get(LAST_MODIFIED)
                .filter(|value| !value.as_bytes().is_empty())
                .cloned()
                .map(ResumeValidator::LastModified)
        })
}

fn validate_response_validator(
    response: &Response,
    expected: &ResumeValidator,
) -> Result<(), DownloadError> {
    let actual = match expected {
        ResumeValidator::StrongEtag(_) => response
            .headers()
            .get(ETAG)
            .filter(|value| is_strong_etag(value)),
        ResumeValidator::LastModified(_) => response.headers().get(LAST_MODIFIED),
    };

    if actual == Some(expected.header_value()) {
        Ok(())
    } else {
        Err(DownloadError::ResumeValidatorMismatch)
    }
}

fn is_strong_etag(value: &HeaderValue) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2
        && bytes.first() == Some(&b'"')
        && bytes.last() == Some(&b'"')
        && !bytes.starts_with(b"W/")
        && !bytes.starts_with(b"w/")
}

async fn load_resume_validator(
    metadata_path: &Path,
    url: &Url,
) -> Result<Option<ResumeValidator>, DownloadError> {
    let bytes = match fs::read(metadata_path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(DownloadError::Io(error)),
    };

    Ok(decode_resume_metadata(&bytes, url))
}

fn decode_resume_metadata(bytes: &[u8], url: &Url) -> Option<ResumeValidator> {
    const FIXED_LENGTH: usize = RESUME_METADATA_MAGIC.len() + 32 + 1 + 4;
    if bytes.len() < FIXED_LENGTH || &bytes[..RESUME_METADATA_MAGIC.len()] != RESUME_METADATA_MAGIC
    {
        return None;
    }

    let hash_start = RESUME_METADATA_MAGIC.len();
    let hash_end = hash_start + 32;
    if bytes[hash_start..hash_end] != url_hash(url) {
        return None;
    }

    let kind = bytes[hash_end];
    let length_start = hash_end + 1;
    let length_end = length_start + 4;
    let validator_length =
        u32::from_be_bytes(bytes[length_start..length_end].try_into().ok()?) as usize;
    if validator_length == 0
        || validator_length > MAX_VALIDATOR_LENGTH
        || bytes.len() != FIXED_LENGTH + validator_length
    {
        return None;
    }

    let value = HeaderValue::from_bytes(&bytes[FIXED_LENGTH..]).ok()?;
    match kind {
        1 if is_strong_etag(&value) => Some(ResumeValidator::StrongEtag(value)),
        2 => Some(ResumeValidator::LastModified(value)),
        _ => None,
    }
}

async fn replace_resume_metadata(
    metadata_path: &Path,
    url: &Url,
    validator: Option<&ResumeValidator>,
) -> Result<(), DownloadError> {
    let Some(validator) = validator else {
        remove_file_if_exists(metadata_path).await?;
        remove_file_if_exists(&metadata_temp_path(metadata_path)).await?;
        return Ok(());
    };

    let bytes = encode_resume_metadata(url, validator)?;
    let temp_path = metadata_temp_path(metadata_path);
    remove_file_if_exists(&temp_path).await?;

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)
        .await?;
    file.write_all(&bytes).await?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    remove_file_if_exists(metadata_path).await?;
    fs::rename(&temp_path, metadata_path).await?;
    Ok(())
}

fn encode_resume_metadata(
    url: &Url,
    validator: &ResumeValidator,
) -> Result<Vec<u8>, DownloadError> {
    let validator_bytes = validator.header_value().as_bytes();
    if validator_bytes.is_empty() || validator_bytes.len() > MAX_VALIDATOR_LENGTH {
        return Err(DownloadError::ResumeValidatorMismatch);
    }
    let validator_length =
        u32::try_from(validator_bytes.len()).map_err(|_| DownloadError::LengthOverflow)?;

    let mut bytes =
        Vec::with_capacity(RESUME_METADATA_MAGIC.len() + 32 + 1 + 4 + validator_bytes.len());
    bytes.extend_from_slice(RESUME_METADATA_MAGIC);
    bytes.extend_from_slice(&url_hash(url));
    bytes.push(validator.kind());
    bytes.extend_from_slice(&validator_length.to_be_bytes());
    bytes.extend_from_slice(validator_bytes);
    Ok(bytes)
}

fn url_hash(url: &Url) -> [u8; 32] {
    Sha256::digest(url.as_str().as_bytes()).into()
}

fn metadata_temp_path(metadata_path: &Path) -> PathBuf {
    let mut path = OsString::from(metadata_path.as_os_str());
    path.push(".tmp");
    PathBuf::from(path)
}

async fn remove_file_if_exists(path: &Path) -> Result<(), DownloadError> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DownloadError::Io(error)),
    }
}

fn parse_satisfied_content_range(
    response: &Response,
) -> Result<SatisfiedContentRange, DownloadError> {
    let value = response
        .headers()
        .get(CONTENT_RANGE)
        .ok_or(DownloadError::InvalidContentRange)?
        .to_str()
        .map_err(|_| DownloadError::InvalidContentRange)?;
    let value = value
        .strip_prefix("bytes ")
        .ok_or(DownloadError::InvalidContentRange)?;
    let (range, total) = value
        .split_once('/')
        .ok_or(DownloadError::InvalidContentRange)?;
    let (start, end) = range
        .split_once('-')
        .ok_or(DownloadError::InvalidContentRange)?;
    let start = start
        .parse::<u64>()
        .map_err(|_| DownloadError::InvalidContentRange)?;
    let end = end
        .parse::<u64>()
        .map_err(|_| DownloadError::InvalidContentRange)?;
    if end < start {
        return Err(DownloadError::InvalidContentRange);
    }

    let total = if total == "*" {
        None
    } else {
        let total = total
            .parse::<u64>()
            .map_err(|_| DownloadError::InvalidContentRange)?;
        if total == 0 || end >= total {
            return Err(DownloadError::InvalidContentRange);
        }
        Some(total)
    };

    Ok(SatisfiedContentRange { start, end, total })
}

fn parse_unsatisfied_content_range(response: &Response) -> Result<u64, DownloadError> {
    let value = response
        .headers()
        .get(CONTENT_RANGE)
        .ok_or(DownloadError::InvalidContentRange)?
        .to_str()
        .map_err(|_| DownloadError::InvalidContentRange)?;
    let total = value
        .strip_prefix("bytes */")
        .ok_or(DownloadError::InvalidContentRange)?;
    total
        .parse::<u64>()
        .map_err(|_| DownloadError::InvalidContentRange)
}

fn response_content_length(response: &Response) -> Result<Option<u64>, DownloadError> {
    response
        .headers()
        .get(CONTENT_LENGTH)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| DownloadError::InvalidContentLength)?
                .parse::<u64>()
                .map_err(|_| DownloadError::InvalidContentLength)
        })
        .transpose()
}

fn validate_reported_total(
    reported: Option<u64>,
    expected: Option<u64>,
) -> Result<(), DownloadError> {
    if let (Some(actual), Some(expected)) = (reported, expected) {
        if actual != expected {
            return Err(DownloadError::ExpectedLengthMismatch { expected, actual });
        }
    }
    Ok(())
}

fn validate_target_path(target_path: &Path) -> Result<(), DownloadError> {
    if target_path.as_os_str().is_empty() || target_path.file_name().is_none() {
        return Err(DownloadError::InvalidTargetPath);
    }
    Ok(())
}

async fn ensure_target_absent(target_path: &Path) -> Result<(), DownloadError> {
    match fs::symlink_metadata(target_path).await {
        Ok(_) => Err(DownloadError::TargetAlreadyExists),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DownloadError::Io(error)),
    }
}

async fn create_target_parent(target_path: &Path) -> Result<(), DownloadError> {
    if let Some(parent) = target_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).await?;
    }
    Ok(())
}

async fn partial_file_length(part_path: &Path) -> Result<u64, DownloadError> {
    match fs::symlink_metadata(part_path).await {
        Ok(metadata) if metadata.file_type().is_file() => Ok(metadata.len()),
        Ok(_) => Err(DownloadError::InvalidPartialFile),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(DownloadError::Io(error)),
    }
}

async fn open_partial_file(part_path: &Path, append: bool) -> Result<File, DownloadError> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    Ok(options.open(part_path).await?)
}

async fn hash_file(
    path: &Path,
    cancellation: &DownloadCancellation,
) -> Result<[u8; 32], DownloadError> {
    let mut file = File::open(path).await?;
    let mut buffer = vec![0_u8; HASH_BUFFER_SIZE];
    let mut hasher = Sha256::new();

    loop {
        ensure_not_cancelled(cancellation)?;
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().into())
}

fn verify_checksum(expected: Option<[u8; 32]>, actual: [u8; 32]) -> Result<(), DownloadError> {
    if let Some(expected) = expected {
        if expected != actual {
            return Err(DownloadError::ChecksumMismatch { expected, actual });
        }
    }
    Ok(())
}

async fn finalize_download(
    part_path: &Path,
    metadata_path: &Path,
    target_path: &Path,
) -> Result<(), DownloadError> {
    ensure_target_absent(target_path).await?;
    fs::rename(part_path, target_path).await?;
    if let Err(error) = remove_file_if_exists(metadata_path).await {
        tracing::warn!(
            path = %metadata_path.display(),
            error = %error,
            "download completed but resume metadata cleanup failed"
        );
    }
    let temp_path = metadata_temp_path(metadata_path);
    if let Err(error) = remove_file_if_exists(&temp_path).await {
        tracing::warn!(
            path = %temp_path.display(),
            error = %error,
            "download completed but temporary resume metadata cleanup failed"
        );
    }
    Ok(())
}

fn ensure_not_cancelled(cancellation: &DownloadCancellation) -> Result<(), DownloadError> {
    if cancellation.is_cancelled() {
        Err(DownloadError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs as std_fs,
        sync::{
            atomic::{AtomicU64, Ordering as AtomicOrdering},
            Arc, Mutex,
        },
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use sha2::{Digest, Sha256};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        task::JoinHandle,
    };

    use super::*;

    const TEST_ETAG: &str = "\"test-etag-v1\"";
    const TEST_ETAG_V2: &str = "\"test-etag-v2\"";
    const TEST_LAST_MODIFIED: &str = "Wed, 21 Oct 2015 07:28:00 GMT";

    #[derive(Debug, Clone, Copy)]
    enum ServerMode {
        Range,
        IgnoreRange,
        Error,
        InvalidRange,
        UnknownRangeTotal,
        MismatchedValidator,
        LastModified,
        NoValidator,
        Truncated,
        Slow,
    }

    #[derive(Clone)]
    struct ServerState {
        body: Arc<Vec<u8>>,
        mode: ServerMode,
        ranges: Arc<Mutex<Vec<Option<String>>>>,
        if_ranges: Arc<Mutex<Vec<Option<String>>>>,
    }

    struct TestServer {
        url: Url,
        ranges: Arc<Mutex<Vec<Option<String>>>>,
        if_ranges: Arc<Mutex<Vec<Option<String>>>>,
        task: JoinHandle<()>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    impl TestServer {
        async fn spawn(body: &[u8], mode: ServerMode) -> Self {
            let ranges = Arc::new(Mutex::new(Vec::new()));
            let if_ranges = Arc::new(Mutex::new(Vec::new()));
            let state = ServerState {
                body: Arc::new(body.to_vec()),
                mode,
                ranges: ranges.clone(),
                if_ranges: if_ranges.clone(),
            };
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let task = tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        break;
                    };
                    let _result = serve_connection(&mut stream, &state).await;
                }
            });

            Self {
                url: Url::parse(&format!("http://{address}/file")).unwrap(),
                ranges,
                if_ranges,
                task,
            }
        }

        fn seen_ranges(&self) -> Vec<Option<String>> {
            self.ranges.lock().unwrap().clone()
        }

        fn seen_if_ranges(&self) -> Vec<Option<String>> {
            self.if_ranges.lock().unwrap().clone()
        }
    }

    #[derive(Debug)]
    struct TestResponse {
        status: &'static str,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
        slow: bool,
    }

    async fn serve_connection(stream: &mut TcpStream, state: &ServerState) -> io::Result<()> {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let bytes_read = stream.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..bytes_read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            if request.len() > 16 * 1024 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "test request headers are too large",
                ));
            }
        }

        let request = String::from_utf8_lossy(&request);
        let method = request.split_whitespace().next().unwrap_or_default();
        let range = request_header(&request, "range");
        let if_range = request_header(&request, "if-range");
        state.ranges.lock().unwrap().push(range.clone());
        state.if_ranges.lock().unwrap().push(if_range);

        let response = match state.mode {
            ServerMode::Error => test_response("503 Service Unavailable", Vec::new(), Vec::new()),
            ServerMode::IgnoreRange => full_response(&state.body),
            ServerMode::InvalidRange if range.is_some() => test_response(
                "206 Partial Content",
                vec![
                    ("Content-Length", state.body.len().to_string()),
                    ("ETag", TEST_ETAG.to_owned()),
                ],
                state.body.as_slice().to_vec(),
            ),
            ServerMode::UnknownRangeTotal if range.is_some() => {
                unknown_total_range_response(&state.body, range.as_deref().unwrap())
            }
            ServerMode::MismatchedValidator if range.is_some() => range_response_with_headers(
                &state.body,
                range.as_deref().unwrap(),
                vec![("ETag", TEST_ETAG_V2.to_owned())],
            ),
            ServerMode::LastModified => match range {
                Some(range) => range_response_with_headers(
                    &state.body,
                    &range,
                    vec![("Last-Modified", TEST_LAST_MODIFIED.to_owned())],
                ),
                None => full_response_with_headers(
                    &state.body,
                    vec![("Last-Modified", TEST_LAST_MODIFIED.to_owned())],
                ),
            },
            ServerMode::NoValidator => match range {
                Some(range) => range_response_with_headers(&state.body, &range, Vec::new()),
                None => full_response_with_headers(&state.body, Vec::new()),
            },
            ServerMode::Truncated => test_response(
                "200 OK",
                vec![
                    ("Content-Length", (state.body.len() + 5).to_string()),
                    ("ETag", TEST_ETAG.to_owned()),
                ],
                state.body.as_slice().to_vec(),
            ),
            ServerMode::Slow => TestResponse {
                status: "200 OK",
                headers: vec![("ETag", TEST_ETAG.to_owned())],
                body: state.body.as_slice().to_vec(),
                slow: true,
            },
            ServerMode::Range
            | ServerMode::InvalidRange
            | ServerMode::UnknownRangeTotal
            | ServerMode::MismatchedValidator => match range {
                Some(range) => range_response(&state.body, &range),
                None => full_response(&state.body),
            },
        };

        write_response(stream, method == "HEAD", response).await
    }

    fn request_header(request: &str, expected_name: &str) -> Option<String> {
        request.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case(expected_name)
                .then(|| value.trim().to_owned())
        })
    }

    async fn write_response(
        stream: &mut TcpStream,
        head_only: bool,
        response: TestResponse,
    ) -> io::Result<()> {
        let mut headers = format!("HTTP/1.1 {}\r\nConnection: close\r\n", response.status);
        for (name, value) in response.headers {
            headers.push_str(name);
            headers.push_str(": ");
            headers.push_str(&value);
            headers.push_str("\r\n");
        }
        headers.push_str("\r\n");
        stream.write_all(headers.as_bytes()).await?;

        if head_only {
            return Ok(());
        }

        if response.slow {
            for (index, chunk) in response.body.chunks(4).enumerate() {
                if index > 0 {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                stream.write_all(chunk).await?;
                stream.flush().await?;
            }
        } else {
            stream.write_all(&response.body).await?;
        }
        Ok(())
    }

    fn full_response(body: &[u8]) -> TestResponse {
        full_response_with_headers(body, vec![("ETag", TEST_ETAG.to_owned())])
    }

    fn full_response_with_headers(
        body: &[u8],
        mut headers: Vec<(&'static str, String)>,
    ) -> TestResponse {
        headers.push(("Content-Length", body.len().to_string()));
        test_response("200 OK", headers, body.to_vec())
    }

    fn range_response(body: &[u8], range: &str) -> TestResponse {
        range_response_with_headers(body, range, vec![("ETag", TEST_ETAG.to_owned())])
    }

    fn range_response_with_headers(
        body: &[u8],
        range: &str,
        mut headers: Vec<(&'static str, String)>,
    ) -> TestResponse {
        let start = range
            .strip_prefix("bytes=")
            .and_then(|value| value.strip_suffix('-'))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap();

        if start >= body.len() {
            headers.push(("Content-Range", format!("bytes */{}", body.len())));
            headers.push(("Content-Length", "0".to_owned()));
            return test_response("416 Range Not Satisfiable", headers, Vec::new());
        }

        let remaining = body[start..].to_vec();
        headers.push((
            "Content-Range",
            format!("bytes {start}-{}/{}", body.len() - 1, body.len()),
        ));
        headers.push(("Content-Length", remaining.len().to_string()));
        test_response("206 Partial Content", headers, remaining)
    }

    fn unknown_total_range_response(body: &[u8], range: &str) -> TestResponse {
        let start = range
            .strip_prefix("bytes=")
            .and_then(|value| value.strip_suffix('-'))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap();
        let remaining = body[start..].to_vec();
        test_response(
            "206 Partial Content",
            vec![
                ("ETag", TEST_ETAG.to_owned()),
                (
                    "Content-Range",
                    format!("bytes {start}-{}/*", body.len() - 1),
                ),
                ("Content-Length", remaining.len().to_string()),
            ],
            remaining,
        )
    }

    fn test_response(
        status: &'static str,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
    ) -> TestResponse {
        TestResponse {
            status,
            headers,
            body,
            slow: false,
        }
    }

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> io::Result<Self> {
            let counter = TEMP_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "wocao-hub-downloader-test-{}-{timestamp}-{counter}",
                std::process::id()
            ));
            std_fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _result = std_fs::remove_dir_all(&self.path);
        }
    }

    fn target(temp: &TempDir) -> PathBuf {
        temp.path().join("downloads").join("installer.bin")
    }

    fn digest(body: &[u8]) -> [u8; 32] {
        Sha256::digest(body).into()
    }

    async fn seed_etag_metadata(url: &Url, target: &Path) {
        seed_metadata(
            url,
            target,
            ResumeValidator::StrongEtag(HeaderValue::from_static(TEST_ETAG)),
        )
        .await;
    }

    async fn seed_last_modified_metadata(url: &Url, target: &Path) {
        seed_metadata(
            url,
            target,
            ResumeValidator::LastModified(HeaderValue::from_static(TEST_LAST_MODIFIED)),
        )
        .await;
    }

    async fn seed_metadata(url: &Url, target: &Path, validator: ResumeValidator) {
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        replace_resume_metadata(&resume_metadata_path_for(target), url, Some(&validator))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn downloads_to_part_then_atomically_renames() {
        let body = b"official installer bytes";
        let server = TestServer::spawn(body, ServerMode::NoValidator).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        let progress = Arc::new(Mutex::new(Vec::new()));
        let captured = progress.clone();

        let outcome = DownloadClient::default()
            .download_with_progress(
                DownloadRequest::new(server.url.clone(), &target)
                    .with_expected_length(body.len() as u64)
                    .with_expected_sha256(digest(body)),
                move |event| captured.lock().unwrap().push(event),
            )
            .await
            .unwrap();

        assert_eq!(fs::read(&target).await.unwrap(), body);
        assert!(!partial_path_for(&target).exists());
        assert!(!resume_metadata_path_for(&target).exists());
        assert_eq!(outcome.total_bytes, body.len() as u64);
        assert_eq!(outcome.transferred_bytes, body.len() as u64);
        assert_eq!(outcome.resumed_from, 0);
        assert_eq!(outcome.sha256, digest(body));
        assert_eq!(
            progress.lock().unwrap().last().unwrap().downloaded_bytes,
            body.len() as u64
        );
    }

    #[tokio::test]
    async fn resumes_an_existing_partial_file_with_range() {
        let body = b"abcdefghijklmnopqrstuvwxyz";
        let server = TestServer::spawn(body, ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(partial_path_for(&target), &body[..7])
            .await
            .unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let outcome = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap();

        assert_eq!(server.seen_ranges(), vec![Some("bytes=7-".to_owned())]);
        assert_eq!(server.seen_if_ranges(), vec![Some(TEST_ETAG.to_owned())]);
        assert_eq!(fs::read(&target).await.unwrap(), body);
        assert_eq!(outcome.resumed_from, 7);
        assert_eq!(outcome.transferred_bytes, (body.len() - 7) as u64);
    }

    #[tokio::test]
    async fn resumes_with_last_modified_when_strong_etag_is_unavailable() {
        let body = b"last modified validator";
        let server = TestServer::spawn(body, ServerMode::LastModified).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(partial_path_for(&target), &body[..5])
            .await
            .unwrap();
        seed_last_modified_metadata(&server.url, &target).await;

        let outcome = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap();

        assert_eq!(outcome.resumed_from, 5);
        assert_eq!(fs::read(&target).await.unwrap(), body);
        assert_eq!(
            server.seen_if_ranges(),
            vec![Some(TEST_LAST_MODIFIED.to_owned())]
        );
    }

    #[tokio::test]
    async fn missing_sidecar_forces_full_download_instead_of_unsafe_append() {
        let body = b"new complete payload";
        let server = TestServer::spawn(body, ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(partial_path_for(&target), b"old bytes")
            .await
            .unwrap();

        let outcome = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap();

        assert_eq!(server.seen_ranges(), vec![None]);
        assert_eq!(server.seen_if_ranges(), vec![None]);
        assert_eq!(outcome.resumed_from, 0);
        assert_eq!(fs::read(&target).await.unwrap(), body);
    }

    #[tokio::test]
    async fn sidecar_for_another_url_forces_full_download() {
        let body = b"payload for current url";
        let server = TestServer::spawn(body, ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(partial_path_for(&target), b"old").await.unwrap();
        let old_url = Url::parse("https://example.invalid/old-installer").unwrap();
        seed_etag_metadata(&old_url, &target).await;

        let outcome = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap();

        assert_eq!(server.seen_ranges(), vec![None]);
        assert_eq!(outcome.resumed_from, 0);
        assert_eq!(fs::read(&target).await.unwrap(), body);
    }

    #[tokio::test]
    async fn mismatched_range_validator_never_appends_to_partial() {
        let body = b"entity version one";
        let server = TestServer::spawn(body, ServerMode::MismatchedValidator).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        let part = partial_path_for(&target);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(&part, b"entity").await.unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let error = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::ResumeValidatorMismatch));
        assert_eq!(fs::read(part).await.unwrap(), b"entity");
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn unknown_total_range_is_rejected_without_appending() {
        let body = b"payload with an unknown range total";
        let server = TestServer::spawn(body, ServerMode::UnknownRangeTotal).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        let part = partial_path_for(&target);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(&part, &body[..4]).await.unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let error = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::InvalidContentRange));
        assert_eq!(fs::read(part).await.unwrap(), &body[..4]);
    }

    #[tokio::test]
    async fn restarts_safely_when_server_ignores_range() {
        let body = b"complete official payload";
        let server = TestServer::spawn(body, ServerMode::IgnoreRange).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(partial_path_for(&target), b"stale prefix")
            .await
            .unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let outcome = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap();

        assert_eq!(fs::read(&target).await.unwrap(), body);
        assert_eq!(outcome.resumed_from, 0);
        assert_eq!(outcome.transferred_bytes, body.len() as u64);
    }

    #[tokio::test]
    async fn cancellation_preserves_partial_and_never_creates_target() {
        let body = b"abcdefghijklmnop";
        let server = TestServer::spawn(body, ServerMode::Slow).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        let cancellation = DownloadCancellation::new();
        let callback_token = cancellation.clone();

        let error = DownloadClient::default()
            .download_with_progress(
                DownloadRequest::new(server.url.clone(), &target).with_cancellation(cancellation),
                move |progress| {
                    if progress.downloaded_bytes > 0 {
                        callback_token.cancel();
                    }
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::Cancelled));
        assert!(!target.exists());
        let partial = fs::read(partial_path_for(&target)).await.unwrap();
        assert!(!partial.is_empty());
        assert!(body.starts_with(&partial));
        assert!(resume_metadata_path_for(&target).exists());
    }

    #[tokio::test]
    async fn cancellation_before_request_does_not_touch_disk_or_network() {
        let server = TestServer::spawn(b"unused", ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        let cancellation = DownloadCancellation::new();
        cancellation.cancel();

        let error = DownloadClient::default()
            .download(
                DownloadRequest::new(server.url.clone(), &target).with_cancellation(cancellation),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::Cancelled));
        assert!(server.seen_ranges().is_empty());
        assert!(!partial_path_for(&target).exists());
        assert!(!resume_metadata_path_for(&target).exists());
    }

    #[tokio::test]
    async fn truncated_body_keeps_part_and_matching_sidecar() {
        let body = b"truncated response bytes";
        let server = TestServer::spawn(body, ServerMode::Truncated).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);

        let error = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::ResponseBody { .. }));
        assert_eq!(fs::read(partial_path_for(&target)).await.unwrap(), body);
        assert!(resume_metadata_path_for(&target).exists());
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn http_failure_preserves_existing_partial() {
        let server = TestServer::spawn(b"unused", ServerMode::Error).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        let part = partial_path_for(&target);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(&part, b"keep me").await.unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let error = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            DownloadError::UnexpectedStatus(StatusCode::SERVICE_UNAVAILABLE)
        ));
        assert_eq!(fs::read(part).await.unwrap(), b"keep me");
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn rejects_invalid_range_without_mutating_partial() {
        let server = TestServer::spawn(b"payload", ServerMode::InvalidRange).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        let part = partial_path_for(&target);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(&part, b"pay").await.unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let error = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::InvalidContentRange));
        assert_eq!(fs::read(part).await.unwrap(), b"pay");
    }

    #[tokio::test]
    async fn checksum_failure_keeps_part_but_invalidates_resume_sidecar() {
        let body = b"downloaded bytes";
        let server = TestServer::spawn(body, ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);

        let error = DownloadClient::default()
            .download(
                DownloadRequest::new(server.url.clone(), &target).with_expected_sha256([7; 32]),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::ChecksumMismatch { .. }));
        assert_eq!(fs::read(partial_path_for(&target)).await.unwrap(), body);
        assert!(!resume_metadata_path_for(&target).exists());
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn retry_replaces_a_complete_part_with_a_bad_checksum() {
        let body = b"verified installer";
        let server = TestServer::spawn(body, ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(partial_path_for(&target), b"corrupt installer!")
            .await
            .unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let outcome = DownloadClient::default()
            .download(
                DownloadRequest::new(server.url.clone(), &target)
                    .with_expected_length(body.len() as u64)
                    .with_expected_sha256(digest(body)),
            )
            .await
            .unwrap();

        assert_eq!(server.seen_ranges(), vec![None]);
        assert_eq!(outcome.resumed_from, 0);
        assert_eq!(fs::read(&target).await.unwrap(), body);
    }

    #[tokio::test]
    async fn rejects_reported_length_mismatch_before_touching_partial() {
        let body = b"0123456789";
        let server = TestServer::spawn(body, ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        let part = partial_path_for(&target);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(&part, &body[..3]).await.unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let error = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target).with_expected_length(11))
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            DownloadError::ExpectedLengthMismatch {
                expected: 11,
                actual: 10
            }
        ));
        assert_eq!(fs::read(part).await.unwrap(), &body[..3]);
    }

    #[tokio::test]
    async fn finalizes_a_part_that_server_confirms_is_complete() {
        let body = b"already complete";
        let server = TestServer::spawn(body, ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(partial_path_for(&target), body).await.unwrap();
        seed_etag_metadata(&server.url, &target).await;

        let outcome = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap();

        assert_eq!(
            server.seen_ranges(),
            vec![Some(format!("bytes={}-", body.len()))]
        );
        assert_eq!(fs::read(&target).await.unwrap(), body);
        assert_eq!(outcome.transferred_bytes, 0);
        assert_eq!(outcome.resumed_from, body.len() as u64);
    }

    #[tokio::test]
    async fn refuses_to_replace_an_existing_target() {
        let server = TestServer::spawn(b"new", ServerMode::Range).await;
        let temp = TempDir::new().unwrap();
        let target = target(&temp);
        fs::create_dir_all(target.parent().unwrap()).await.unwrap();
        fs::write(&target, b"existing").await.unwrap();

        let error = DownloadClient::default()
            .download(DownloadRequest::new(server.url.clone(), &target))
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::TargetAlreadyExists));
        assert_eq!(fs::read(target).await.unwrap(), b"existing");
        assert!(server.seen_ranges().is_empty());
    }

    #[tokio::test]
    async fn content_length_validates_status_and_header() {
        let body = b"known length";
        let server = TestServer::spawn(body, ServerMode::Range).await;
        let length = DownloadClient::default()
            .content_length(server.url.clone())
            .await
            .unwrap();
        assert_eq!(length, Some(body.len() as u64));
    }
}
