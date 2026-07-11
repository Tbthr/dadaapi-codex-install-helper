use regex::Regex;

use crate::DiagnosticsError;

const REDACTED_CREDENTIAL: &str = "[REDACTED_CREDENTIAL]";
const REDACTED_URL: &str = "[REDACTED_URL]";
const REDACTED_PATH: &str = "[REDACTED_PATH]";
const REDACTED_NETWORK: &str = "[REDACTED_NETWORK]";
const REDACTED_OPAQUE: &str = "[REDACTED_OPAQUE]";

#[derive(Clone)]
pub struct Redactor {
    credential_patterns: Vec<Regex>,
    url_patterns: Vec<Regex>,
    path_patterns: Vec<Regex>,
    network_patterns: Vec<Regex>,
    opaque_patterns: Vec<Regex>,
}

impl Redactor {
    pub fn new() -> Result<Self, DiagnosticsError> {
        Ok(Self {
            credential_patterns: compile_patterns(&[
                r"(?is)-----BEGIN [A-Z0-9 ]*(?:PRIVATE KEY|CERTIFICATE)-----.*?-----END [A-Z0-9 ]*(?:PRIVATE KEY|CERTIFICATE)-----",
                r"(?i)\b(?:bearer|basic)\s+[a-z0-9._~+/=-]{4,}",
                r#"(?i)(?:\"|')?\b(?:authorization|proxy-authorization|password|passwd|pwd|secret|token|access[_-]?token|refresh[_-]?token|api[_-]?key|private[_-]?key|route[_-]?key|subscription(?:[_-]?url)?|credential|auth|cookie|set-cookie|session(?:[_-]?id)?|username|user|server|host|hostname|endpoint|node|address|proxy)\b(?:\"|')?\s*[:=]\s*(?:\"[^\"]*\"|'[^']*'|[^\s,;}\]]+)"#,
            ])?,
            url_patterns: compile_patterns(&[
                r#"(?i)\b(?:https?|wss?|file|vless|vmess|hysteria2|hy2|ss|ssr|trojan|socks5?)://[^\s<>()\[\]{}\"']+"#,
            ])?,
            path_patterns: compile_patterns(&[
                r#"(?i)(?:[a-z]:\\+|\\\\+)[^\r\n\t\"'<>]+"#,
                r#"(?i)%[a-z_]+%\\+[^\r\n\t\"'<>]+"#,
                r#"(?:/(?:Users|home|root|private|tmp|Volumes|var/folders)/)[^\r\n\t\"'<>]+"#,
                r#"(?:~[/\\])[^\r\n\t\"'<>]+"#,
            ])?,
            network_patterns: compile_patterns(&[
                r"(?i)\b[a-z0-9](?:[a-z0-9-]{0,62}\.)+[a-z]{2,24}\b",
                r"\b(?:25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})(?:\.(?:25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})){3}\b",
                r"(?i)(?:\[[0-9a-f:]{2,}\]|\b[0-9a-f]{1,4}(?::[0-9a-f]{0,4}){2,7}\b)",
                r"::1\b",
                r"(?i)\blocalhost(?::[0-9]{1,5})?\b",
                r"(?i)\b[a-z0-9][a-z0-9-]{1,62}:[0-9]{1,5}\b",
                r"(?i)\b[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,24}\b",
                r"(?i)\b(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}\b",
                r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b",
                r"(?i)\bS-1-(?:[0-9]+-){1,14}[0-9]+\b",
            ])?,
            opaque_patterns: compile_patterns(&[
                r"\b[0-9a-fA-F]{32,}\b",
                r"\b[A-Za-z0-9_+/-]{36,}={0,2}\b",
            ])?,
        })
    }

    /// Applies both credential and privacy redaction. Calling this method more
    /// than once is intentional and idempotent, so export can repeat the pass.
    pub fn redact(&self, input: &str) -> String {
        let mut output = normalize_control_characters(input);

        // Whole URLs are removed before credential assignments so user info,
        // query parameters and fragments never survive as partial strings.
        output = replace_all(&self.url_patterns, &output, REDACTED_URL);
        output = replace_all(&self.credential_patterns, &output, REDACTED_CREDENTIAL);
        output = replace_all(&self.path_patterns, &output, REDACTED_PATH);
        output = replace_all(&self.network_patterns, &output, REDACTED_NETWORK);
        replace_all(&self.opaque_patterns, &output, REDACTED_OPAQUE)
    }

    pub(crate) fn audit(&self, input: &str) -> Result<(), DiagnosticsError> {
        let groups = [
            &self.credential_patterns,
            &self.url_patterns,
            &self.path_patterns,
            &self.network_patterns,
            &self.opaque_patterns,
        ];

        if groups
            .into_iter()
            .flat_map(|patterns| patterns.iter())
            .any(|pattern| pattern.is_match(input))
        {
            return Err(DiagnosticsError::SensitiveDataDetected);
        }

        Ok(())
    }
}

fn compile_patterns(patterns: &[&str]) -> Result<Vec<Regex>, DiagnosticsError> {
    patterns
        .iter()
        .map(|pattern| {
            Regex::new(pattern).map_err(|_| DiagnosticsError::RedactorInitializationFailed)
        })
        .collect()
}

fn replace_all(patterns: &[Regex], input: &str, replacement: &str) -> String {
    patterns.iter().fold(input.to_owned(), |current, pattern| {
        pattern.replace_all(&current, replacement).into_owned()
    })
}

fn normalize_control_characters(input: &str) -> String {
    input
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => character,
            character if character.is_control() => '\u{fffd}',
            character => character,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_urls_paths_and_network_identifiers() {
        let redactor = Redactor::new().expect("redactor should initialize");
        let input = concat!(
            "authorization: Bearer secret-value ",
            "url=https://user:pass@example.com/path?token=hidden ",
            "source 203.0.113.8 via proxy.example.net ",
            "mac=/Users/alice/Library/Application Support/Wocao Hub/recovery.json ",
            "win=C:\\Users\\alice\\AppData\\Roaming\\Wocao Hub\\recovery.json ",
            "route=vless://11111111-2222-3333-4444-555555555555@example.net:443"
        );

        let redacted = redactor.redact(input);

        for secret in [
            "secret-value",
            "user:pass",
            "example.com",
            "alice",
            "Application Support",
            "203.0.113.8",
            "proxy.example.net",
            "11111111-2222-3333-4444-555555555555",
            "vless://",
        ] {
            assert!(!redacted.contains(secret), "secret survived: {secret}");
        }
        assert!(redacted.contains(REDACTED_CREDENTIAL));
        assert!(redacted.contains(REDACTED_URL));
        assert!(redacted.contains(REDACTED_PATH));
        assert!(redacted.contains(REDACTED_NETWORK));
        redactor
            .audit(&redacted)
            .expect("redacted text should pass audit");
    }

    #[test]
    fn redaction_is_idempotent() {
        let redactor = Redactor::new().expect("redactor should initialize");
        let once = redactor.redact("token=secret https://example.com/private");
        let twice = redactor.redact(&once);

        assert_eq!(once, twice);
    }

    #[test]
    fn removes_pem_and_long_opaque_values() {
        let redactor = Redactor::new().expect("redactor should initialize");
        let redacted = redactor.redact(concat!(
            "-----BEGIN PRIVATE KEY-----\n",
            "abcdefghijklmnopqrstuvwxyz0123456789+/=\n",
            "-----END PRIVATE KEY-----\n",
            "sha=0123456789abcdef0123456789abcdef0123456789abcdef"
        ));

        assert!(!redacted.contains("PRIVATE KEY-----"));
        assert!(!redacted.contains("0123456789abcdef"));
        redactor
            .audit(&redacted)
            .expect("redacted text should pass audit");
    }

    #[test]
    fn redacts_unqualified_node_hosts_and_ipv6_loopback() {
        let redactor = Redactor::new().expect("redactor should initialize");
        let redacted = redactor.redact(concat!(
            "server=private-node endpoint=relayhost:443 loopback=::1 proxy=localhost:7890 ",
            "mac=00:1A:2B:3C:4D:5E device=550e8400-e29b-41d4-a716-446655440000 ",
            "sid=S-1-5-21-1004336348-1177238915-682003330-512"
        ));

        for secret in [
            "private-node",
            "relayhost",
            "::1",
            "localhost",
            "00:1A:2B:3C:4D:5E",
            "550e8400-e29b-41d4-a716-446655440000",
            "S-1-5-21-1004336348-1177238915-682003330-512",
        ] {
            assert!(!redacted.contains(secret), "secret survived: {secret}");
        }
        redactor
            .audit(&redacted)
            .expect("redacted text should pass audit");
    }
}
