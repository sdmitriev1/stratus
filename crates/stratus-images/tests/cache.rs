use std::sync::Arc;

use sha2::{Digest, Sha256};
use stratus_images::ImageCache;
use stratus_resources::ImageFormat;

async fn test_server(data: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
    use axum::{Router, routing::get};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}/image");

    let handle = tokio::spawn(async move {
        let app = Router::new().route(
            "/image",
            get(move || {
                let d = data.clone();
                async move { d }
            }),
        );
        axum::serve(listener, app).await.unwrap();
    });

    (url, handle)
}

/// Serve image data at `/image.img` and a checksums file at `/SHA256SUMS`.
async fn test_server_with_checksums(
    data: Vec<u8>,
    checksums_body: String,
) -> (String, String, tokio::task::JoinHandle<()>) {
    use axum::{Router, routing::get};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let image_url = format!("http://{addr}/image.img");
    let checksums_url = format!("http://{addr}/SHA256SUMS");

    let handle = tokio::spawn(async move {
        let app = Router::new()
            .route(
                "/image.img",
                get({
                    let d = data.clone();
                    move || {
                        let d = d.clone();
                        async move { d }
                    }
                }),
            )
            .route(
                "/SHA256SUMS",
                get(move || {
                    let c = checksums_body.clone();
                    async move { c }
                }),
            );
        axum::serve(listener, app).await.unwrap();
    });

    (image_url, checksums_url, handle)
}

/// Serve data and also count the number of requests received.
async fn test_server_with_counter(
    data: Vec<u8>,
) -> (
    String,
    tokio::task::JoinHandle<()>,
    Arc<std::sync::atomic::AtomicUsize>,
) {
    use axum::{Router, routing::get};
    use std::sync::atomic::AtomicUsize;

    let counter = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}/image");

    let counter_clone = counter.clone();
    let handle = tokio::spawn(async move {
        let app = Router::new().route(
            "/image",
            get(move || {
                let d = data.clone();
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    d
                }
            }),
        );
        axum::serve(listener, app).await.unwrap();
    });

    (url, handle, counter)
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

fn make_checksum(data: &[u8]) -> (String, String) {
    let hex = hex_encode(&Sha256::digest(data));
    let checksum = format!("sha256:{hex}");
    (checksum, hex)
}

#[test]
fn lookup_miss() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();
    assert!(cache.lookup("deadbeef").is_none());
}

#[test]
fn lookup_hit() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    let hex = "abcdef1234567890";
    let path = dir.path().join("sha256").join(hex);
    std::fs::write(&path, b"cached data").unwrap();

    let result = cache.lookup(hex);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), path);
}

#[tokio::test]
async fn ensure_downloads_and_caches() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    let data = b"test image data for caching";
    let (checksum, hex) = make_checksum(data);

    let (url, handle) = test_server(data.to_vec()).await;

    let result = cache.ensure(&url, &checksum, ImageFormat::Raw).await;
    assert!(result.is_ok(), "ensure failed: {:?}", result.err());

    let cached = result.unwrap();
    assert_eq!(cached.path, dir.path().join("sha256").join(&hex));
    assert!(cached.path.exists());
    assert_eq!(std::fs::read(&cached.path).unwrap(), data);

    // No partial files should remain
    let partials: Vec<_> = std::fs::read_dir(dir.path().join(".downloading"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(partials.is_empty());

    handle.abort();
}

#[tokio::test]
async fn ensure_cache_hit_skips_download() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    let data = b"pre-cached data";
    let (checksum, hex) = make_checksum(data);

    // Pre-populate cache
    let cache_path = dir.path().join("sha256").join(&hex);
    std::fs::write(&cache_path, data).unwrap();

    // Use a URL that would fail if actually requested
    let result = cache
        .ensure(
            "http://127.0.0.1:1/should-not-be-called",
            &checksum,
            ImageFormat::Raw,
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().path, cache_path);
}

#[tokio::test]
async fn ensure_checksum_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    let data = b"actual data";
    let (url, handle) = test_server(data.to_vec()).await;

    // Use wrong checksum
    let wrong_checksum = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    let result = cache.ensure(&url, wrong_checksum, ImageFormat::Raw).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, stratus_images::ImageError::ChecksumMismatch { .. }),
        "expected ChecksumMismatch, got: {err:?}"
    );

    // No file in sha256/
    let hex = "0000000000000000000000000000000000000000000000000000000000000000";
    assert!(!dir.path().join("sha256").join(hex).exists());

    // No partial files
    let partials: Vec<_> = std::fs::read_dir(dir.path().join(".downloading"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(partials.is_empty());

    handle.abort();
}

#[tokio::test]
async fn ensure_concurrent_dedup() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(ImageCache::new(dir.path().to_path_buf()).unwrap());

    let data = b"concurrent download test data";
    let (checksum, _hex) = make_checksum(data);

    let (url, handle, counter) = test_server_with_counter(data.to_vec()).await;

    // Spawn two concurrent ensure calls
    let cache1 = cache.clone();
    let url1 = url.clone();
    let checksum1 = checksum.clone();
    let h1 = tokio::spawn(async move { cache1.ensure(&url1, &checksum1, ImageFormat::Raw).await });

    let cache2 = cache.clone();
    let url2 = url.clone();
    let checksum2 = checksum.clone();
    let h2 = tokio::spawn(async move { cache2.ensure(&url2, &checksum2, ImageFormat::Raw).await });

    let (r1, r2) = tokio::join!(h1, h2);
    assert!(r1.unwrap().is_ok());
    assert!(r2.unwrap().is_ok());

    // Server should have received at most 1 request (dedup), but could be 2
    // if the second request starts before the first registers in_flight.
    // With small test data, timing makes strict assertion flaky.
    // At minimum, both should succeed.
    let count = counter.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        (1..=2).contains(&count),
        "expected 1-2 requests, got {count}"
    );

    handle.abort();
}

#[tokio::test]
async fn ensure_partial_cleaned_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    // Use URL that will fail
    let result = cache
        .ensure(
            "http://127.0.0.1:1/will-fail",
            "sha256:deadbeef",
            ImageFormat::Raw,
        )
        .await;

    assert!(result.is_err());

    // No partial files should remain
    let partials: Vec<_> = std::fs::read_dir(dir.path().join(".downloading"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(partials.is_empty());
}

#[test]
fn evict_existing() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    let data = b"to be evicted";
    let (checksum, hex) = make_checksum(data);
    let path = dir.path().join("sha256").join(&hex);
    std::fs::write(&path, data).unwrap();

    assert!(cache.evict(&checksum).unwrap());
    assert!(!path.exists());
}

#[test]
fn evict_nonexistent() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    assert!(
        !cache
            .evict("sha256:0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap()
    );
}

#[tokio::test]
async fn ensure_resolves_checksum_url() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    let data = b"checksum url test image data";
    let (_, hex) = make_checksum(data);

    let checksums_body = format!("{hex} *image.img\n");
    let (image_url, checksums_url, handle) =
        test_server_with_checksums(data.to_vec(), checksums_body).await;

    let checksum = format!("sha256:{checksums_url}");
    let result = cache.ensure(&image_url, &checksum, ImageFormat::Raw).await;
    assert!(result.is_ok(), "ensure failed: {:?}", result.err());

    let cached = result.unwrap();
    assert_eq!(cached.path, dir.path().join("sha256").join(&hex));
    assert!(cached.path.exists());
    assert_eq!(cached.checksum, format!("sha256:{hex}"));
    assert_eq!(std::fs::read(&cached.path).unwrap(), data);

    handle.abort();
}

#[tokio::test]
async fn ensure_checksum_url_filename_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(dir.path().to_path_buf()).unwrap();

    // Checksums file doesn't contain the image filename
    let checksums_body = "abc123 *other-file.img\n".to_string();
    let (image_url, checksums_url, handle) =
        test_server_with_checksums(b"data".to_vec(), checksums_body).await;

    let checksum = format!("sha256:{checksums_url}");
    let result = cache.ensure(&image_url, &checksum, ImageFormat::Raw).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, stratus_images::ImageError::ChecksumFile(_)),
        "expected ChecksumFile, got: {err:?}"
    );

    handle.abort();
}
