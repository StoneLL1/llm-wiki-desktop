use llm_wiki_desktop_lib::services::import_v2::{
    domain_limiter::DomainLimiter,
    url_policy::{PrivateTargetGrant, UrlPolicy},
    web_fetch::{WebFetchContent, WebFetchPolicy, WebFetchResume, WebFetchService},
};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpListener},
    sync::{Arc, Mutex},
    thread,
};
#[test]
fn fetch_policy_is_bounded() {
    let p = WebFetchPolicy::default();
    assert_eq!(p.max_attempts_per_route, 2);
    assert!(p.max_redirects <= 8 && p.max_response_bytes <= 16 * 1024 * 1024);
}
#[tokio::test]
async fn sensitive_domains_are_single_concurrency() {
    let l = DomainLimiter::default();
    let first = l.acquire("mp.weixin.qq.com", true).await.unwrap();
    let blocked = tokio::time::timeout(
        std::time::Duration::from_millis(10),
        l.acquire("mp.weixin.qq.com", true),
    )
    .await;
    assert!(blocked.is_err());
    drop(first);
    assert!(l.acquire("mp.weixin.qq.com", true).await.is_ok());
}

fn server(response: String) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let join = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 2048];
        let _ = stream.read(&mut request);
        stream.write_all(response.as_bytes()).unwrap();
    });
    (port, join)
}
fn grant(port: u16) -> PrivateTargetGrant {
    PrivateTargetGrant {
        item_id: "item".into(),
        scheme: "http".into(),
        host: "127.0.0.1".into(),
        port,
        resolved_ips: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(1),
    }
}

#[tokio::test]
async fn controlled_server_streams_bounded_public_artifact_with_private_grant() {
    let body = "<html><article>complete fixture body</article></html>";
    let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
    let (port, join) = server(response);
    let target = UrlPolicy
        .normalize_for_session(&format!("http://127.0.0.1:{port}/a?token=secret#frag"))
        .unwrap();
    let artifact = WebFetchService
        .fetch(
            target,
            &UrlPolicy,
            &WebFetchPolicy::default(),
            Some(&grant(port)),
            "item",
            |_| {},
            || false,
        )
        .await
        .unwrap();
    join.join().unwrap();
    assert_eq!(artifact.bytes, body.as_bytes());
    assert!(
        !artifact.final_public_url.contains("secret")
            && !artifact.final_public_url.contains("frag")
    );
}

#[tokio::test]
async fn controlled_server_can_stream_a_response_directly_to_disk() {
    let body = "streamed response body";
    let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
    let (port, join) = server(response);
    let target = UrlPolicy
        .normalize_for_session(&format!("http://127.0.0.1:{port}/stream"))
        .unwrap();
    let root = std::env::temp_dir().join(format!("web-fetch-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let destination = root.join("response.bin");
    let artifact = WebFetchService
        .fetch_to_file(
            target,
            &UrlPolicy,
            &WebFetchPolicy::default(),
            Some(&grant(port)),
            "item",
            &destination,
            |_| {},
            || false,
        )
        .await
        .unwrap();
    join.join().unwrap();
    assert!(artifact.bytes.is_empty());
    assert_eq!(artifact.byte_len, body.len() as u64);
    assert_eq!(std::fs::read(&destination).unwrap(), body.as_bytes());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn resumable_fetch_verifies_range_identity_and_appends_to_the_partial() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let join = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let read = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert!(request.to_ascii_lowercase().contains("range: bytes=4-"));
        assert!(
            request.contains("If-Range: \"media-v1\"")
                || request.contains("if-range: \"media-v1\"")
        );
        stream
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Type: application/octet-stream\r\nContent-Length: 6\r\nContent-Range: bytes 4-9/10\r\nETag: \"media-v1\"\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\nefghij",
            )
            .unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("partial.bin");
    std::fs::write(&destination, b"abcd").unwrap();
    let target = UrlPolicy
        .normalize_for_session(&format!("http://127.0.0.1:{port}/media"))
        .unwrap();
    let checkpoints = Arc::new(Mutex::new(Vec::new()));
    let observed = checkpoints.clone();
    let artifact = WebFetchService
        .fetch_to_file_resumable(
            target,
            &UrlPolicy,
            &WebFetchPolicy {
                content: WebFetchContent::TemporaryMedia,
                ..WebFetchPolicy::default()
            },
            Some(&grant(port)),
            "item",
            &destination,
            Some(&WebFetchResume {
                downloaded_bytes: 4,
                total_bytes: Some(10),
                etag: Some("\"media-v1\"".into()),
                last_modified: None,
                partial_sha256: format!("{:x}", Sha256::digest(b"abcd")),
            }),
            |_| {},
            move |checkpoint| {
                observed.lock().unwrap().push(checkpoint);
                Ok(())
            },
            || false,
        )
        .await
        .unwrap();
    join.join().unwrap();
    assert_eq!(artifact.byte_len, 10);
    assert_eq!(std::fs::read(destination).unwrap(), b"abcdefghij");
    let checkpoint = checkpoints.lock().unwrap().last().cloned().unwrap();
    assert_eq!(checkpoint.downloaded_bytes, 10);
    assert!(checkpoint.range_supported);
}

#[tokio::test]
async fn resumable_fetch_restarts_safely_when_the_server_ignores_range() {
    let body = "replacement body";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nETag: \"media-v2\"\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let (port, join) = server(response);
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("partial.bin");
    std::fs::write(&destination, b"obsolete").unwrap();
    let target = UrlPolicy
        .normalize_for_session(&format!("http://127.0.0.1:{port}/media"))
        .unwrap();
    WebFetchService
        .fetch_to_file_resumable(
            target,
            &UrlPolicy,
            &WebFetchPolicy {
                content: WebFetchContent::TemporaryMedia,
                ..WebFetchPolicy::default()
            },
            Some(&grant(port)),
            "item",
            &destination,
            Some(&WebFetchResume {
                downloaded_bytes: 8,
                total_bytes: None,
                etag: Some("\"media-v1\"".into()),
                last_modified: None,
                partial_sha256: format!("{:x}", Sha256::digest(b"obsolete")),
            }),
            |_| {},
            |_| Ok(()),
            || false,
        )
        .await
        .unwrap();
    join.join().unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), body.as_bytes());
}

#[tokio::test]
async fn completed_verified_partial_activates_without_an_invalid_range_request() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("partial.bin");
    std::fs::write(&destination, b"complete").unwrap();
    let target = UrlPolicy
        .normalize_for_session("https://example.com/media.bin")
        .unwrap();
    let artifact = WebFetchService
        .fetch_to_file_resumable(
            target,
            &UrlPolicy,
            &WebFetchPolicy {
                content: WebFetchContent::TemporaryMedia,
                ..WebFetchPolicy::default()
            },
            None,
            "item",
            &destination,
            Some(&WebFetchResume {
                downloaded_bytes: 8,
                total_bytes: Some(8),
                etag: Some("\"media-v1\"".into()),
                last_modified: None,
                partial_sha256: format!("{:x}", Sha256::digest(b"complete")),
            }),
            |_| {},
            |_| Ok(()),
            || false,
        )
        .await
        .unwrap();
    assert_eq!(artifact.byte_len, 8);
}

#[tokio::test]
async fn truncated_partial_content_is_never_promoted_as_complete() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let join = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Type: application/octet-stream\r\nContent-Length: 2\r\nContent-Range: bytes 4-9/10\r\nETag: \"media-v1\"\r\nConnection: close\r\n\r\nef",
            )
            .unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("partial.bin");
    std::fs::write(&destination, b"abcd").unwrap();
    let target = UrlPolicy
        .normalize_for_session(&format!("http://127.0.0.1:{port}/media"))
        .unwrap();
    let error = WebFetchService
        .fetch_to_file_resumable(
            target,
            &UrlPolicy,
            &WebFetchPolicy {
                content: WebFetchContent::TemporaryMedia,
                ..WebFetchPolicy::default()
            },
            Some(&grant(port)),
            "item",
            &destination,
            Some(&WebFetchResume {
                downloaded_bytes: 4,
                total_bytes: Some(10),
                etag: Some("\"media-v1\"".into()),
                last_modified: None,
                partial_sha256: format!("{:x}", Sha256::digest(b"abcd")),
            }),
            |_| {},
            |_| Ok(()),
            || false,
        )
        .await
        .unwrap_err();
    join.join().unwrap();
    assert_eq!(error.code, "IMPORT_WEB_PARTIAL_INVALID");
    assert_eq!(std::fs::read(destination).unwrap(), b"abcd");
}

#[tokio::test]
async fn redirect_to_a_different_private_origin_requires_a_new_grant() {
    let(destination,destination_join)=server("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".into());
    let redirect=format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{destination}/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let (source, source_join) = server(redirect);
    let target = UrlPolicy
        .normalize_for_session(&format!("http://127.0.0.1:{source}/start"))
        .unwrap();
    let error = WebFetchService
        .fetch(
            target,
            &UrlPolicy,
            &WebFetchPolicy::default(),
            Some(&grant(source)),
            "item",
            |_| {},
            || false,
        )
        .await
        .unwrap_err();
    source_join.join().unwrap();
    assert_eq!(error.code, "IMPORT_V2_PRIVATE_TARGET_BLOCKED");
    drop(destination_join);
}

#[tokio::test]
async fn unknown_length_body_still_obeys_stream_limit() {
    let body = "x".repeat(1024);
    let response =
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n{body}");
    let (port, join) = server(response);
    let target = UrlPolicy
        .normalize_for_session(&format!("http://127.0.0.1:{port}/large"))
        .unwrap();
    let mut policy = WebFetchPolicy::default();
    policy.max_response_bytes = 64;
    let error = WebFetchService
        .fetch(
            target,
            &UrlPolicy,
            &policy,
            Some(&grant(port)),
            "item",
            |_| {},
            || false,
        )
        .await
        .unwrap_err();
    join.join().unwrap();
    assert_eq!(error.code, "IMPORT_V2_RESPONSE_TOO_LARGE");
}
