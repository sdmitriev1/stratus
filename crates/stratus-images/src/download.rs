use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::ImageError;

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

/// Download a file from `url` to `dest`, streaming chunks while computing SHA-256.
/// Returns the hex-encoded SHA-256 digest of the downloaded content.
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> Result<String, ImageError> {
    let mut response = client.get(url).send().await?.error_for_status()?;

    let total_size = response.content_length();
    let mut file = tokio::fs::File::create(dest).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_logged: u64 = 0;

    while let Some(chunk) = response.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if downloaded - last_logged >= 10 * 1024 * 1024 {
            if let Some(total) = total_size {
                info!(
                    "download progress: {:.1}MB / {:.1}MB ({:.0}%)",
                    downloaded as f64 / 1_048_576.0,
                    total as f64 / 1_048_576.0,
                    downloaded as f64 / total as f64 * 100.0
                );
            } else {
                info!(
                    "download progress: {:.1}MB",
                    downloaded as f64 / 1_048_576.0
                );
            }
            last_logged = downloaded;
        }
    }

    file.flush().await?;
    drop(file);

    let digest = hasher.finalize();
    Ok(hex_encode(&digest))
}
