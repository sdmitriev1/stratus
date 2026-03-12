use sha2::{Digest, Sha256};
use stratus_images::download::download_file;

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

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

#[tokio::test]
async fn download_computes_correct_sha256() {
    let data = b"hello world, this is test data for sha256 verification";
    let expected = hex_encode(&Sha256::digest(data));

    let (url, handle) = test_server(data.to_vec()).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("downloaded");

    let client = reqwest::Client::new();
    let digest = download_file(&client, &url, &dest).await.unwrap();

    assert_eq!(digest, expected);
    handle.abort();
}

#[tokio::test]
async fn download_creates_file_with_correct_content() {
    let data = b"file content that should be preserved exactly";

    let (url, handle) = test_server(data.to_vec()).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("downloaded");

    let client = reqwest::Client::new();
    download_file(&client, &url, &dest).await.unwrap();

    let content = std::fs::read(&dest).unwrap();
    assert_eq!(content, data);
    handle.abort();
}

#[tokio::test]
async fn download_bad_url_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("downloaded");

    let client = reqwest::Client::new();
    let result = download_file(&client, "http://127.0.0.1:1/nonexistent", &dest).await;

    assert!(result.is_err());
}
