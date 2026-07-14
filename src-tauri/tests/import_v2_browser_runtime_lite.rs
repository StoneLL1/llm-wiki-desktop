use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

fn run_fixture(html: &str, url: &str) -> (String, String, String) {
    let root = tempfile::tempdir().unwrap();
    let staging = root.path().join("staging");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("fetched.html"), html).unwrap();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let mut child = Command::new("node")
        .arg(repo.join("capabilities/browser-runtime-lite/runner/index.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let rpc = serde_json::json!({"jsonrpc":"2.0","id":"r1","method":"import.execute","params":{"protocolVersion":"2","requestId":"r1","sessionId":"s","itemId":"i","taskId":"t","operation":"extract","input":{"kind":"url","displayName":"fixture","locator":url,"normalizedLocator":url,"sourceIdentity":null},"projectRoot":root.path(),"stagingRoot":"staging","chainedInput":"fetched.html"}});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{rpc}\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8(output.stdout).unwrap(),
        fs::read_to_string(staging.join("candidate.md")).unwrap_or_default(),
        fs::read_to_string(staging.join("source.html")).unwrap_or_default(),
    )
}

#[test]
fn nested_wechat_body_is_complete_sanitized_and_remote_urls_stay_on_stdio() {
    let html = r#"<h1 id="activity-name">Nested</h1><span id="js_name">Author</span><div id="js_content"><section><div><p>First complete paragraph in the article.</p></div><div><p>Second complete paragraph after a nested div.</p><img data-src="https://mmbiz.qpic.cn/a.jpg?signature=very-secret"></div></section><script>alert(1)</script></div>"#;
    let (stdout, markdown, snapshot) = run_fixture(html, "https://mp.weixin.qq.com/s/public-id");
    assert!(stdout.contains("import.remoteAsset") && stdout.contains("very-secret"));
    assert!(markdown.contains("First complete") && markdown.contains("Second complete"));
    assert!(
        !markdown.contains("very-secret")
            && !snapshot.contains("very-secret")
            && !snapshot.contains("script")
    );
}

#[test]
fn challenge_returns_typed_failure_without_artifacts() {
    let (stdout, _, _) = run_fixture(
        "<body>环境异常，请完成安全验证 captcha</body>",
        "https://mp.weixin.qq.com/s/id",
    );
    assert!(stdout.contains("IMPORT_WEB_CHALLENGE_DETECTED"));
}
