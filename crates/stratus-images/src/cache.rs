use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::sync::{Mutex, watch};
use tracing::{info, warn};

use stratus_resources::ImageFormat;

use crate::ImageError;
use crate::download::download_file;
use crate::verify::{parse_checksum, validate_image};

#[derive(Debug)]
pub struct CachedImage {
    pub path: PathBuf,
    pub checksum: String,
}

type InflightResult = Option<Result<PathBuf, String>>;

pub struct ImageCache {
    cache_dir: PathBuf,
    client: reqwest::Client,
    in_flight: Mutex<HashMap<String, watch::Receiver<InflightResult>>>,
}

impl ImageCache {
    /// Create a new ImageCache rooted at `cache_dir`.
    /// Creates `sha256/` and `.downloading/` subdirectories.
    pub fn new(cache_dir: PathBuf) -> Result<Self, ImageError> {
        std::fs::create_dir_all(cache_dir.join("sha256"))?;
        std::fs::create_dir_all(cache_dir.join(".downloading"))?;

        let client = reqwest::Client::builder()
            .user_agent(concat!("stratus/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(ImageError::Download)?;

        Ok(Self {
            cache_dir,
            client,
            in_flight: Mutex::new(HashMap::new()),
        })
    }

    /// Check if an image with the given checksum is already cached.
    /// `checksum` is the hex digest (without algorithm prefix).
    pub fn lookup(&self, checksum: &str) -> Option<PathBuf> {
        let path = self.cache_dir.join("sha256").join(checksum);
        if path.exists() { Some(path) } else { None }
    }

    /// Ensure an image is cached. Downloads if not present.
    /// Uses concurrent deduplication: if another task is already downloading
    /// the same image, waits for it to complete.
    pub async fn ensure(
        &self,
        url: &str,
        checksum: &str,
        format: ImageFormat,
    ) -> Result<CachedImage, ImageError> {
        let (_algo, hex) = parse_checksum(checksum)?;

        // Fast path: already cached
        if let Some(path) = self.lookup(hex) {
            return Ok(CachedImage {
                path,
                checksum: checksum.to_string(),
            });
        }

        // Check if another download is in flight
        {
            let in_flight = self.in_flight.lock().await;
            if let Some(rx) = in_flight.get(hex) {
                let mut rx = rx.clone();
                drop(in_flight);
                // Wait for the other download to complete
                let _ = rx.wait_for(|v| v.is_some()).await;
                let result = rx.borrow();
                match result.as_ref().unwrap() {
                    Ok(path) => {
                        return Ok(CachedImage {
                            path: path.clone(),
                            checksum: checksum.to_string(),
                        });
                    }
                    Err(e) => {
                        return Err(ImageError::InvalidImage(format!(
                            "concurrent download failed: {e}"
                        )));
                    }
                }
            }
        }

        // We're the downloader. Set up the watch channel.
        let (tx, rx) = watch::channel(None);
        {
            let mut in_flight = self.in_flight.lock().await;
            // Double-check: another task may have started between our check and acquiring the lock
            if let Some(existing_rx) = in_flight.get(hex) {
                let mut rx = existing_rx.clone();
                drop(in_flight);
                let _ = rx.wait_for(|v| v.is_some()).await;
                let result = rx.borrow();
                match result.as_ref().unwrap() {
                    Ok(path) => {
                        return Ok(CachedImage {
                            path: path.clone(),
                            checksum: checksum.to_string(),
                        });
                    }
                    Err(e) => {
                        return Err(ImageError::InvalidImage(format!(
                            "concurrent download failed: {e}"
                        )));
                    }
                }
            }
            in_flight.insert(hex.to_string(), rx);
        }

        let result = self.do_download(url, hex, format).await;

        // Notify waiters and clean up
        match &result {
            Ok(cached) => {
                let _ = tx.send(Some(Ok(cached.path.clone())));
            }
            Err(e) => {
                let _ = tx.send(Some(Err(e.to_string())));
            }
        }
        {
            let mut in_flight = self.in_flight.lock().await;
            in_flight.remove(hex);
        }

        result
    }

    async fn do_download(
        &self,
        url: &str,
        hex: &str,
        format: ImageFormat,
    ) -> Result<CachedImage, ImageError> {
        let partial = self
            .cache_dir
            .join(".downloading")
            .join(format!("{hex}.partial"));
        let final_path = self.cache_dir.join("sha256").join(hex);

        info!(url, checksum = hex, "downloading image");

        match self
            .download_and_verify(url, hex, format, &partial, &final_path)
            .await
        {
            Ok(cached) => Ok(cached),
            Err(e) => {
                // Clean up partial file on failure
                if let Err(rm_err) = tokio::fs::remove_file(&partial).await
                    && rm_err.kind() != std::io::ErrorKind::NotFound
                {
                    warn!("failed to clean up partial file: {rm_err}");
                }
                Err(e)
            }
        }
    }

    async fn download_and_verify(
        &self,
        url: &str,
        hex: &str,
        format: ImageFormat,
        partial: &Path,
        final_path: &Path,
    ) -> Result<CachedImage, ImageError> {
        let actual_hex = download_file(&self.client, url, partial).await?;

        if actual_hex != hex {
            return Err(ImageError::ChecksumMismatch {
                expected: hex.to_string(),
                actual: actual_hex,
            });
        }

        // Validate image format via qemu-img (best effort — skip if qemu-img not available)
        match validate_image(partial, format).await {
            Ok(info) => {
                info!(
                    format = info.format,
                    virtual_size = info.virtual_size,
                    actual_size = info.actual_size,
                    "image validated"
                );
            }
            Err(ImageError::QemuImg(ref msg)) if msg.contains("failed to run qemu-img") => {
                warn!("qemu-img not available, skipping image validation");
            }
            Err(e) => return Err(e),
        }

        // Atomic rename into cache
        tokio::fs::rename(partial, final_path).await?;

        info!(path = %final_path.display(), "image cached");

        Ok(CachedImage {
            path: final_path.to_path_buf(),
            checksum: format!("sha256:{hex}"),
        })
    }

    /// Remove a cached image. Returns true if the file existed.
    pub fn evict(&self, checksum: &str) -> Result<bool, ImageError> {
        let (_algo, hex) = parse_checksum(checksum)?;
        let path = self.cache_dir.join("sha256").join(hex);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(ImageError::Io(e)),
        }
    }
}
