pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::now_rfc3339;

    #[test]
    fn returns_rfc3339_timestamp() {
        let timestamp = now_rfc3339();

        assert!(timestamp.contains('T'));
        assert!(timestamp.ends_with("+00:00") || timestamp.ends_with('Z'));
    }
}
