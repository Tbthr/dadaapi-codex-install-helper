use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use downloader::ms_store::resolve_chatgpt_msix_url_direct;
use serde::Serialize;
use shared_types::CpuArchitecture;
use url::Url;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    generated_at: String,
    packages: Packages,
}

#[derive(Debug, Serialize)]
struct Packages {
    arm64: Package,
    x64: Package,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Package {
    url: String,
    expires_at: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let output_path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("output path is required")?;
    let (arm64, x64) = tokio::try_join!(
        resolve_chatgpt_msix_url_direct(CpuArchitecture::Arm64),
        resolve_chatgpt_msix_url_direct(CpuArchitecture::X64),
    )
    .context("could not resolve Microsoft Store packages")?;
    let manifest = Manifest {
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        packages: Packages {
            arm64: package(arm64)?,
            x64: package(x64)?,
        },
    };
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).context("could not create output directory")?;
    }
    fs::write(
        output_path,
        serde_json::to_vec_pretty(&manifest).context("could not encode manifest")?,
    )
    .context("could not write manifest")?;
    Ok(())
}

fn package(url: Url) -> Result<Package> {
    let expires_at = url
        .query_pairs()
        .find_map(|(key, value)| (key == "P1").then_some(value.into_owned()))
        .context("Microsoft URL has no expiry")?
        .parse::<i64>()
        .context("Microsoft URL expiry is invalid")?;
    let expires_at = DateTime::from_timestamp(expires_at, 0)
        .context("Microsoft URL expiry is outside the supported range")?
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    Ok(Package {
        url: url.to_string(),
        expires_at,
    })
}
