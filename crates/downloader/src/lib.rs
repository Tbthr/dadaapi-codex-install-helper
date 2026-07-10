use reqwest::Client;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("download request failed: {0}")]
    Request(#[from] reqwest::Error),
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
    pub async fn content_length(&self, url: Url) -> Result<Option<u64>, DownloadError> {
        let response = self.client.head(url).send().await?.error_for_status()?;
        Ok(response.content_length())
    }
}
