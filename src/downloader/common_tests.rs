use std::sync::Arc;

use indicatif::ProgressBar;
use reqwest::Client;

use super::{common::download_file, PackagePayload};

#[tokio::test]
async fn download_file_should_complete_successfully() {
    let mut server = mockito::Server::new_async().await;
    let body = "0123456789";

    let _m = server
        .mock("GET", "/file")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    let file_path = dir.join("payload.bin");

    let pb = Arc::new(ProgressBar::hidden());

    let payload = PackagePayload {
        file_name: "payload.bin".to_string(),
        url: format!("{}/file", server.url()),
        size: 10,
        sha256: None,
    };

    let client = Client::builder().build().unwrap();

    let result = download_file(&client, &payload, &file_path, &pb)
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(pb.position(), 10);

    let data = tokio::fs::read(&file_path).await.unwrap();
    assert_eq!(data, body.as_bytes());
}
