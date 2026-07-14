use llm_wiki_desktop_lib::services::import_v2::{
    domain_limiter::DomainLimiter,
    url_policy::{PrivateTargetGrant, UrlPolicy},
    web_fetch::{WebFetchPolicy, WebFetchService},
};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpListener},
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
    assert_eq!(error.code, "IMPORT_V2_URL_REJECTED");
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
