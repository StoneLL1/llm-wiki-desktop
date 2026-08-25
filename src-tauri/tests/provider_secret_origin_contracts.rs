use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use llm_wiki_desktop_lib::models::llm::{
    LlmProviderConfig, LlmProviderKind, ProviderCredentialBinding,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::url_policy::UrlPolicy;
use llm_wiki_desktop_lib::services::{LlmService, SecretService};

const MAX_TEST_HTTP_REQUEST_BYTES: usize = 64 * 1024;
const TEST_SERVER_SHUTDOWN_SENTINEL: &[u8] = b"llm-wiki-test-server-shutdown\n";

#[derive(Debug, PartialEq, Eq)]
enum TestConnection {
    Empty,
    HttpRequest,
    Shutdown,
}

struct TestServerWorker {
    shutdown: Arc<AtomicBool>,
    address: SocketAddr,
    worker: Option<thread::JoinHandle<()>>,
}

impl TestServerWorker {
    fn signal_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Ok(mut stream) = TcpStream::connect(self.address) {
            let _ = stream.write_all(TEST_SERVER_SHUTDOWN_SENTINEL);
            let _ = stream.flush();
        }
    }

    fn join(mut self) -> thread::Result<()> {
        self.signal_shutdown();
        self.worker.take().unwrap().join()
    }
}

impl Drop for TestServerWorker {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.signal_shutdown();
            let _ = self.worker.take().unwrap().join();
        }
    }
}

fn config(provider: LlmProviderKind, base_url: impl Into<String>) -> LlmProviderConfig {
    LlmProviderConfig {
        provider,
        model: "test-model".into(),
        base_url: base_url.into(),
        context_window: 8_000,
        enabled: true,
    }
}

fn context(name: &str) -> ProjectContext {
    ProjectContext::new(
        name,
        std::env::temp_dir().join(format!("provider-binding-{name}")),
    )
}

fn approved_binding(
    context: &ProjectContext,
    provider: LlmProviderKind,
    origin: &str,
) -> ProviderCredentialBinding {
    let config_id = uuid::Uuid::new_v4().to_string();
    let revision = 1;
    ProviderCredentialBinding {
        credential_account_id: SecretService::provider_binding_account_id(
            context, provider, &config_id, origin, revision,
        )
        .unwrap(),
        config_id,
        provider_kind: provider,
        canonical_origin: origin.into(),
        approved_at: Some("2026-08-18T00:00:00Z".into()),
        revision,
    }
}

fn spawn_server(response: Option<String>) -> (String, Arc<AtomicUsize>, TestServerWorker) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let (count, worker) = spawn_bound_server(listener, response);
    (format!("http://{address}"), count, worker)
}

fn spawn_bound_server(
    listener: TcpListener,
    response: Option<String>,
) -> (Arc<AtomicUsize>, TestServerWorker) {
    let address = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_requested = shutdown.clone();
    let worker = thread::spawn(move || loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                match read_test_connection(&mut stream, &shutdown_requested) {
                    Ok(TestConnection::Empty) | Err(_) => continue,
                    Ok(TestConnection::Shutdown) => return,
                    Ok(TestConnection::HttpRequest) => {}
                }
                observed.fetch_add(1, Ordering::SeqCst);
                if let Some(response) = response.as_deref() {
                    stream.write_all(response.as_bytes()).unwrap();
                    stream.flush().unwrap();
                }
                return;
            }
            Err(_) => return,
        }
    });
    (
        count,
        TestServerWorker {
            shutdown,
            address,
            worker: Some(worker),
        },
    )
}

fn read_test_connection(
    reader: &mut impl Read,
    shutdown_requested: &AtomicBool,
) -> std::io::Result<TestConnection> {
    let mut request = Vec::with_capacity(4_096);
    let mut chunk = [0_u8; 4_096];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            if request.is_empty() {
                return Ok(TestConnection::Empty);
            }
            if shutdown_requested.load(Ordering::SeqCst) && request == TEST_SERVER_SHUTDOWN_SENTINEL
            {
                return Ok(TestConnection::Shutdown);
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "test HTTP request ended before its declared body",
            ));
        }
        let next_len = request.len().checked_add(read).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "test HTTP request length overflowed",
            )
        })?;
        if next_len > MAX_TEST_HTTP_REQUEST_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "test HTTP request exceeded the bounded fixture limit",
            ));
        }
        request.extend_from_slice(&chunk[..read]);

        if shutdown_requested.load(Ordering::SeqCst) && request == TEST_SERVER_SHUTDOWN_SENTINEL {
            return Ok(TestConnection::Shutdown);
        }
        if shutdown_requested.load(Ordering::SeqCst)
            && TEST_SERVER_SHUTDOWN_SENTINEL.starts_with(&request)
        {
            continue;
        }
        if let Some(expected_len) = expected_http_request_len(&request)? {
            if request.len() >= expected_len {
                return Ok(TestConnection::HttpRequest);
            }
        }
    }
}

fn expected_http_request_len(request: &[u8]) -> std::io::Result<Option<usize>> {
    let Some(header_start) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Ok(None);
    };
    let header_len = header_start + 4;
    let header = std::str::from_utf8(&request[..header_len]).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "test HTTP request headers were not UTF-8",
        )
    })?;
    let mut content_length = None;
    for line in header.split("\r\n").skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if !name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        let parsed = value.trim().parse::<usize>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "test HTTP request had an invalid Content-Length",
            )
        })?;
        if content_length.is_some_and(|existing| existing != parsed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "test HTTP request had conflicting Content-Length values",
            ));
        }
        content_length = Some(parsed);
    }
    let expected_len = header_len
        .checked_add(content_length.unwrap_or_default())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "test HTTP request length overflowed",
            )
        })?;
    if expected_len > MAX_TEST_HTTP_REQUEST_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "test HTTP request exceeded the bounded fixture limit",
        ));
    }
    Ok(Some(expected_len))
}

#[test]
fn request_sentinel_ignores_bare_tcp_probes() {
    let response = "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let (server, request_count, worker) = spawn_server(Some(response.into()));
    let address = server.strip_prefix("http://").unwrap();

    TcpStream::connect(address).unwrap();
    let mut request = TcpStream::connect(address).unwrap();
    request
        .write_all(b"GET /sentinel HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();

    worker.join().unwrap();
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
}

#[test]
fn request_reader_waits_for_a_fragmented_declared_body() {
    let first =
        std::io::Cursor::new(b"POST / HTTP/1.1\r\nContent-Length: 11\r\n\r\nhello".to_vec());
    let second = std::io::Cursor::new(b" world".to_vec());
    let mut fragmented = first.chain(second);

    assert_eq!(
        read_test_connection(&mut fragmented, &AtomicBool::new(false)).unwrap(),
        TestConnection::HttpRequest
    );
}

#[test]
fn explicit_server_shutdown_is_not_counted_as_a_request() {
    let (_server, request_count, worker) = spawn_server(None);

    worker.join().unwrap();
    assert_eq!(request_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn attacker_request_count_stays_zero_for_unapproved_http_and_https() {
    let (attacker, request_count, worker) = spawn_server(None);
    let port = attacker.rsplit(':').next().unwrap().to_string();

    for malicious in [
        attacker,
        format!("https://127.0.0.1:{port}"),
        "http://attacker.invalid".into(),
        "https://attacker.invalid".into(),
    ] {
        let error = LlmService
            .complete(
                &config(LlmProviderKind::OpenAi, malicious),
                Some("must-not-leak"),
                "hello",
            )
            .await
            .unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("must-not-leak"));
        assert!(matches!(
            error.code.as_str(),
            "LLM_OFFICIAL_ORIGIN_REQUIRED" | "LLM_DESTINATION_BLOCKED"
        ));
    }

    worker.join().unwrap();
    assert_eq!(request_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn redirect_never_carries_secret_to_redirect_target() {
    for status in [301, 302, 307, 308] {
        let (target, target_count, target_worker) = spawn_server(None);
        let response = format!(
            "HTTP/1.1 {status} Redirect\r\nLocation: {target}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let (source, source_count, source_worker) = spawn_server(Some(response));

        let error = LlmService
            .complete(
                &config(LlmProviderKind::Custom, source),
                Some("bound-secret"),
                "hello",
            )
            .await
            .unwrap_err();

        source_worker.join().unwrap();
        target_worker.join().unwrap();
        assert_eq!(error.code, "LLM_REDIRECT_REJECTED");
        assert_eq!(source_count.load(Ordering::SeqCst), 1);
        assert_eq!(target_count.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn same_origin_redirect_never_replays_secret() {
    for status in [301, 302, 307, 308] {
        let response = format!(
            "HTTP/1.1 {status} Redirect\r\nLocation: /same-origin-steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let (source, source_count, source_worker) = spawn_server(Some(response));
        let error = LlmService
            .complete(
                &config(LlmProviderKind::Custom, source),
                Some("same-origin-secret"),
                "hello",
            )
            .await
            .unwrap_err();

        source_worker.join().unwrap();
        assert_eq!(error.code, "LLM_REDIRECT_REJECTED");
        assert_eq!(source_count.load(Ordering::SeqCst), 1);
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("same-origin-secret"));
    }
}

#[tokio::test]
async fn https_loopback_target_is_rejected_before_tls_or_secret_send() {
    let (target, request_count, worker) = spawn_server(None);
    let https_target = target.replacen("http://", "https://", 1);
    let error = LlmService
        .complete(
            &config(LlmProviderKind::Custom, https_target),
            Some("tls-secret"),
            "hello",
        )
        .await
        .unwrap_err();

    worker.join().unwrap();
    assert_eq!(error.code, "LLM_DESTINATION_BLOCKED");
    assert_eq!(request_count.load(Ordering::SeqCst), 0);
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("tls-secret"));
}

#[tokio::test]
async fn ipv6_literal_loopback_ollama_probe_connects_without_bracketed_dns_lookup() {
    let Ok(listener) = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)) else {
        return;
    };
    let address = listener.local_addr().unwrap();
    let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{\"models\":[]}";
    let (request_count, worker) = spawn_bound_server(listener, Some(response.into()));
    let endpoint = format!("http://[::1]:{}", address.port());

    let (base_url, model_count) = LlmService
        .probe_ollama(&config(LlmProviderKind::Ollama, &endpoint))
        .await
        .unwrap();

    worker.join().unwrap();
    assert_eq!(base_url, endpoint);
    assert_eq!(model_count, 0);
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
}

#[test]
fn rejects_0_0_0_0_and_private_lan() {
    let policy = UrlPolicy;
    assert!(policy
        .normalize_provider_endpoint("http://0.0.0.0:11434")
        .is_err());
    assert!(policy
        .normalize_provider_endpoint("http://192.168.1.4:11434")
        .is_err());

    let https_private = policy
        .normalize_provider_endpoint("https://192.168.1.4")
        .unwrap();
    let private = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 4));
    assert!(policy
        .validate_provider_resolution(&https_private, &[private], private, false)
        .is_err());
}

#[test]
fn rejects_169_254_169_254_and_mapped_ipv6() {
    let policy = UrlPolicy;
    let metadata = policy
        .normalize_provider_endpoint("https://169.254.169.254/latest/meta-data")
        .unwrap();
    let metadata_ip = IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254));
    assert!(policy
        .validate_provider_resolution(&metadata, &[metadata_ip], metadata_ip, false)
        .is_err());

    let mapped = policy
        .normalize_provider_endpoint("https://[::ffff:127.0.0.1]")
        .unwrap();
    let mapped_ip = IpAddr::V6(Ipv6Addr::from(0xffff_7f00_0001_u128));
    assert!(policy
        .validate_provider_resolution(&mapped, &[mapped_ip], mapped_ip, false)
        .is_err());
}

#[test]
fn rejects_reserved_documentation_and_deprecated_transition_ranges() {
    for ip in [
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
        IpAddr::V4(Ipv4Addr::new(198, 51, 100, 10)),
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)),
        "2001:db8::10".parse().unwrap(),
        "2002:c000:0201::".parse().unwrap(),
        "::192.0.2.10".parse().unwrap(),
        "64:ff9b::a9fe:a9fe".parse().unwrap(),
        "64:ff9b:1::a9fe:a9fe".parse().unwrap(),
        "2001:2::10".parse().unwrap(),
        "3fff::10".parse().unwrap(),
        "5f00::10".parse().unwrap(),
    ] {
        let endpoint = match ip {
            IpAddr::V4(ip) => format!("https://{ip}"),
            IpAddr::V6(ip) => format!("https://[{ip}]"),
        };
        let target = UrlPolicy.normalize_provider_endpoint(&endpoint).unwrap();
        assert!(UrlPolicy
            .validate_provider_resolution(&target, &[ip], ip, false)
            .is_err());
    }

    let outside_documentation_prefix: IpAddr = "3fff:1000::1".parse().unwrap();
    let target = UrlPolicy
        .normalize_provider_endpoint("https://[3fff:1000::1]")
        .unwrap();
    assert!(UrlPolicy
        .validate_provider_resolution(
            &target,
            &[outside_documentation_prefix],
            outside_documentation_prefix,
            false,
        )
        .is_ok());
}

#[test]
fn loopback_policy_rejects_mixed_dns_and_rebinding_and_accepts_exact_localhost() {
    let policy = UrlPolicy;
    let localhost = policy
        .normalize_provider_endpoint("http://LOCALHOST.:11434")
        .unwrap();
    let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let public = IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34));

    assert!(policy
        .validate_provider_resolution(&localhost, &[loopback], loopback, false)
        .is_ok());
    assert!(policy
        .validate_provider_resolution(&localhost, &[loopback, public], loopback, false)
        .is_err());
    assert!(policy
        .validate_provider_resolution(&localhost, &[loopback], public, false)
        .is_err());
    assert_eq!(
        policy.canonical_origin(&localhost),
        "http://localhost:11434"
    );
}

#[test]
fn legacy_credentials_are_untrusted_by_bound_lookup() {
    let context = context("legacy");
    let secrets = SecretService::memory();
    secrets
        .set(LlmProviderKind::OpenAi, "legacy-global-key")
        .unwrap();
    let binding = approved_binding(&context, LlmProviderKind::OpenAi, "https://api.openai.com");

    assert_eq!(secrets.get_bound(&context, &binding).unwrap(), None);
    assert_eq!(
        secrets.get(LlmProviderKind::OpenAi).unwrap().as_deref(),
        Some("legacy-global-key")
    );
}

#[test]
fn project_a_b_and_origin_changes_never_reuse_bound_credentials() {
    let project_a = context("project-a");
    let project_b = context("project-b");
    let secrets = SecretService::memory();
    let binding = approved_binding(
        &project_a,
        LlmProviderKind::Custom,
        "https://provider-a.example",
    );
    secrets
        .set_bound(&project_a, &binding, "project-a-secret")
        .unwrap();

    assert!(secrets.get_bound(&project_b, &binding).is_err());
    let mut changed_origin = binding.clone();
    changed_origin.canonical_origin = "https://provider-b.example".into();
    assert!(secrets.get_bound(&project_a, &changed_origin).is_err());
    let mut changed_id = binding.clone();
    changed_id.config_id = uuid::Uuid::new_v4().to_string();
    assert!(secrets.get_bound(&project_a, &changed_id).is_err());
}

#[test]
fn tampered_origin_with_same_id_and_revision_is_rejected_before_secret_store() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".app")).unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("tampered-origin", root.path().to_path_buf());
    let settings = llm_wiki_desktop_lib::services::SettingsService::with_config_dir(
        config_dir.path().to_path_buf(),
    );
    let secrets = SecretService::memory();
    let displayed_origin = "https://displayed.example";
    let attacker_origin = "https://attacker.example";
    let mut binding = approved_binding(&context, LlmProviderKind::Custom, displayed_origin);
    binding.approved_at = None;
    settings
        .save_provider_with_binding(
            &context,
            config(LlmProviderKind::Custom, displayed_origin),
            binding.clone(),
        )
        .unwrap();

    binding.canonical_origin = attacker_origin.into();
    binding.credential_account_id = SecretService::provider_binding_account_id(
        &context,
        LlmProviderKind::Custom,
        &binding.config_id,
        attacker_origin,
        binding.revision,
    )
    .unwrap();
    settings
        .save_provider_with_binding(
            &context,
            config(LlmProviderKind::Custom, attacker_origin),
            binding.clone(),
        )
        .unwrap();

    let error = LlmService::approve_and_store_secret(
        &context,
        &secrets,
        LlmProviderKind::Custom,
        &binding.config_id,
        binding.revision,
        displayed_origin,
        "must-not-store",
    )
    .unwrap_err();

    assert_eq!(error.code, "PROVIDER_CREDENTIAL_BINDING_CHANGED");
    assert_eq!(
        secrets.get_account(&binding.credential_account_id).unwrap(),
        None
    );
}

#[test]
fn official_origins_and_normalization_are_fail_closed() {
    for (provider, official) in [
        (LlmProviderKind::OpenAi, "https://api.openai.com"),
        (LlmProviderKind::Anthropic, "https://api.anthropic.com"),
        (
            LlmProviderKind::Google,
            "https://generativelanguage.googleapis.com",
        ),
    ] {
        assert!(LlmService::validate_config(&config(provider, official)).is_ok());
        assert!(
            LlmService::validate_config(&config(provider, "https://attacker.example")).is_err()
        );
        assert!(LlmService::validate_config(&config(
            provider,
            format!("{official}.attacker.example")
        ))
        .is_err());
    }

    let target = UrlPolicy
        .normalize_provider_endpoint("https://Example.COM.:443/v1")
        .unwrap();
    assert_eq!(UrlPolicy.canonical_origin(&target), "https://example.com");
}
