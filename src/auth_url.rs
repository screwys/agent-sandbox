use std::collections::HashSet;

const KEY: &str = "\"authUrl\"";
const MAX_BUFFER: usize = 256 * 1024;
const RETAIN_BUFFER: usize = 64 * 1024;

#[derive(Debug, Default)]
pub struct AuthUrlScanner {
    buffer: String,
    seen: HashSet<String>,
}

impl AuthUrlScanner {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));

        let mut urls = Vec::new();
        let mut offset = 0;
        while let Some(relative_key) = self.buffer[offset..].find(KEY) {
            let key = offset + relative_key;
            let mut value_at = skip_ws(&self.buffer, key + KEY.len());
            if self.buffer.as_bytes().get(value_at) != Some(&b':') {
                offset = key + 1;
                continue;
            }
            value_at = skip_ws(&self.buffer, value_at + 1);
            if self.buffer.as_bytes().get(value_at) != Some(&b'"') {
                offset = key + 1;
                continue;
            }

            let Some((url, end)) = parse_json_string(&self.buffer, value_at) else {
                break;
            };
            if should_open_auth_url(&url) && self.seen.insert(url.clone()) {
                urls.push(url);
            }
            offset = end;
        }

        self.trim_buffer();
        urls
    }

    fn trim_buffer(&mut self) {
        if self.buffer.len() <= MAX_BUFFER {
            return;
        }
        let mut drain_to = self.buffer.len().saturating_sub(RETAIN_BUFFER);
        while drain_to < self.buffer.len() && !self.buffer.is_char_boundary(drain_to) {
            drain_to += 1;
        }
        self.buffer.drain(..drain_to);
    }
}

fn should_open_auth_url(url: &str) -> bool {
    url.starts_with("https://") && !url.chars().any(char::is_control)
}

fn skip_ws(text: &str, mut index: usize) -> usize {
    let bytes = text.as_bytes();
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    index
}

fn parse_json_string(text: &str, quote_at: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(quote_at) != Some(&b'"') {
        return None;
    }

    let mut out = String::new();
    let mut index = quote_at + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Some((out, index + 1)),
            b'\\' => {
                index += 1;
                let escaped = *bytes.get(index)?;
                match escaped {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let start = index + 1;
                        let end = start + 4;
                        let hex = text.get(start..end)?;
                        let codepoint = u16::from_str_radix(hex, 16).ok()?;
                        if let Some(ch) = char::from_u32(codepoint as u32) {
                            out.push(ch);
                        }
                        index += 4;
                    }
                    _ => return None,
                }
                index += 1;
            }
            _ => {
                let ch = text[index..].chars().next()?;
                out.push(ch);
                index += ch.len_utf8();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_chunked_json_auth_urls_once() {
        let mut scanner = AuthUrlScanner::default();

        assert!(scanner.push(br#"{"id":1,"result":{"auth"#).is_empty());
        assert_eq!(
            scanner.push(br#"Url":"https:\/\/auth.openai.com\/oauth\/authorize?x=1\u0026y=2"}}"#),
            vec!["https://auth.openai.com/oauth/authorize?x=1&y=2"]
        );
        assert!(
            scanner
                .push(br#"{"authUrl":"https://auth.openai.com/oauth/authorize?x=1&y=2"}"#)
                .is_empty()
        );
    }

    #[test]
    fn ignores_non_https_and_control_character_urls() {
        let mut scanner = AuthUrlScanner::default();

        assert!(
            scanner
                .push(br#"{"authUrl":"http://example.com"}"#)
                .is_empty()
        );
        assert!(
            scanner
                .push(b"{\"authUrl\":\"https://example.com/line\\nnext\"}")
                .is_empty()
        );
    }

    #[test]
    fn waits_for_complete_json_string() {
        let mut scanner = AuthUrlScanner::default();

        assert!(
            scanner
                .push(br#"{"authUrl":"https://auth.openai.com/oauth"#)
                .is_empty()
        );
        assert_eq!(
            scanner.push(br#"/authorize"}"#),
            vec!["https://auth.openai.com/oauth/authorize"]
        );
    }
}
