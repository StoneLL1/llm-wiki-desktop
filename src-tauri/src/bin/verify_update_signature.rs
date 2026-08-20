use std::env;
use std::fs;
use std::process;

use base64::Engine;
use minisign_verify::{PublicKey, Signature};

fn verify(public_key_base64: &str, artifact: &str, signature_file: &str) -> Result<(), String> {
    let public_key_text = base64::engine::general_purpose::STANDARD
        .decode(public_key_base64)
        .map_err(|_| "updater public key is not valid base64".to_string())?;
    let public_key_text = String::from_utf8(public_key_text)
        .map_err(|_| "updater public key is not UTF-8".to_string())?;
    let public_key = PublicKey::decode(&public_key_text)
        .map_err(|_| "updater public key is not a valid minisign key".to_string())?;
    let signature_base64 = fs::read_to_string(signature_file)
        .map_err(|error| format!("cannot read updater signature: {error}"))?;
    let signature_text = base64::engine::general_purpose::STANDARD
        .decode(signature_base64.trim())
        .map_err(|_| "updater signature is not valid base64".to_string())?;
    let signature_text = String::from_utf8(signature_text)
        .map_err(|_| "updater signature is not UTF-8".to_string())?;
    let signature = Signature::decode(&signature_text)
        .map_err(|_| "updater signature is not a valid minisign signature".to_string())?;
    let bytes =
        fs::read(artifact).map_err(|error| format!("cannot read updater artifact: {error}"))?;
    public_key
        .verify(&bytes, &signature, true)
        .map_err(|_| "updater signature verification failed".to_string())
}

fn main() {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.len() < 3 || arguments.len() % 2 == 0 {
        eprintln!(
            "usage: verify_update_signature <public-key-base64> <artifact> <signature> [...]"
        );
        process::exit(2);
    }
    let public_key = &arguments[0];
    for pair in arguments[1..].chunks_exact(2) {
        if let Err(error) = verify(public_key, &pair[0], &pair[1]) {
            eprintln!("{error}: {}", pair[0]);
            process::exit(1);
        }
    }
    println!(
        "verified {} updater signature(s)",
        (arguments.len() - 1) / 2
    );
}

#[cfg(test)]
mod tests {
    use super::verify;
    use std::fs;

    const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXkgNDIyNjBGMTg0MkI0RTgxRgpSV1FmNkxSQ0dBOWk1M21sWWVjTzRJelQ1MVRHUHB2V3VjTlNDaDFDQk0wUVRhTG43M1k3R0ZPMwo=";
    const WRONG_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDBEMjc0RUU4OEFCOTA2NTYKUldSV0JybUs2RTRuRGN2UlZENGdNY1FxUE1aZ2F1Y2NBSnpiZ25qTmRtdURURzYrMitUdms4SUEK";
    const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUldRZjZMUkNHQTlpNTlTTE9GeHo2Tnh2QVNYREplUnR1Wnlrd1FlcGJERUd0ODdpZzFCTnBXYVZXdU5ybTczWWlJaUpicTcxV2krZFA5ZUtMOE9DMzUxdndJYXNTU2JYeHdBPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNTU1Nzc5OTY2CWZpbGU6dGVzdApRdEtNWFd5WWN3ZHBaQWxQRjd0RTJFTkprUmQxdWp2S2psajFtOVJ0SFRCblpQYTVXS1U1dVdSczVHb1A1TS9WcUU4MVFGdU1LSTVrL1NmTlFVYU9BQT09Cg==";

    fn compact(value: &str) -> String {
        value
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect()
    }

    #[test]
    fn accepts_the_matching_signature_and_rejects_changed_bytes_or_key() {
        let directory = tempfile::tempdir().expect("create fixture directory");
        let artifact = directory.path().join("artifact.bin");
        let signature = directory.path().join("artifact.bin.sig");
        fs::write(&artifact, b"test").expect("write artifact");
        fs::write(&signature, compact(SIGNATURE)).expect("write signature");
        assert!(verify(
            &compact(PUBLIC_KEY),
            artifact.to_str().unwrap(),
            signature.to_str().unwrap()
        )
        .is_ok());
        fs::write(&artifact, b"tampered").expect("tamper artifact");
        assert!(verify(
            &compact(PUBLIC_KEY),
            artifact.to_str().unwrap(),
            signature.to_str().unwrap()
        )
        .is_err());
        fs::write(&artifact, b"test").expect("restore artifact");
        assert!(verify(
            WRONG_PUBLIC_KEY,
            artifact.to_str().unwrap(),
            signature.to_str().unwrap()
        )
        .is_err());
    }
}
