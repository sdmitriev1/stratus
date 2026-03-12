pub mod cache;
pub mod download;
pub mod verify;
pub use cache::ImageCache;

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("download failed: {0}")]
    Download(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("missing checksum")]
    MissingChecksum,
    #[error("unsupported checksum algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("invalid image: {0}")]
    InvalidImage(String),
    #[error("qemu-img failed: {0}")]
    QemuImg(String),
    #[error("image has backing file reference: {0}")]
    BackingFile(String),
}
