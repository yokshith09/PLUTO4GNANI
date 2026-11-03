//! Which links Pluto will hand to the operating system. Pure, so it is tested here and the shell
//! only calls it before opening anything in the default browser.
//!
//! Only `http`, `https` and `mailto` pass. `file:`, `javascript:`, `data:`, custom app schemes
//! and anything that is not plainly one link (control characters, whitespace, over 2048
//! characters) are refused, because a note or imported text could contain any of them.

use crate::error::{AppError, Result};

pub const URL_MAX_LEN: usize = 2048;

fn refuse(message: &str) -> AppError {
    AppError::validation("url", message)
}

/// Check `url` and return it trimmed, ready to open.
pub fn validate_external_url(url: &str) -> Result<String> {
    let url = url.trim();
    if url.is_empty() {
        return Err(refuse("That link is empty."));
    }
    if url.chars().count() > URL_MAX_LEN {
        return Err(refuse("That link is too long to open."));
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(refuse("That link contains characters Pluto won't open."));
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        return Err(refuse(
            "Pluto can open web links (http or https) and email links (mailto) only.",
        ));
    };
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => {
            let after_slashes = rest
                .strip_prefix("//")
                .ok_or_else(|| refuse("That web link is missing its address."))?;
            let host = after_slashes
                .split(['/', '?', '#'])
                .next()
                .unwrap_or_default();
            // `user:pass@host` is a classic way to disguise where a link goes.
            if host.is_empty() || host.contains('@') || host.starts_with(':') {
                return Err(refuse("That web link is missing its address."));
            }
        }
        "mailto" => {
            let address = rest.split(['?', '#']).next().unwrap_or_default();
            if address.is_empty() || rest.starts_with("//") {
                return Err(refuse("That email link has no address."));
            }
        }
        _ => {
            return Err(refuse(
                "Pluto can open web links (http or https) and email links (mailto) only.",
            ));
        }
    }
    Ok(url.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn web_and_mail_links_pass() {
        for ok in [
            "http://example.com",
            "https://example.com/a/b?c=d#e",
            "HTTPS://Example.com",
            "https://localhost:8080/x",
            "https://[::1]:3000/",
            "mailto:someone@example.com",
            "mailto:a@b.co?subject=Hi%20there",
            "  https://example.com  ",
        ] {
            assert!(validate_external_url(ok).is_ok(), "{ok}");
        }
        assert_eq!(
            validate_external_url("  https://example.com ").unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn every_other_scheme_is_refused() {
        for bad in [
            "file:///etc/passwd",
            "FILE:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "ftp://example.com",
            "ms-settings:privacy",
            "vscode://file/x",
            "tel:123",
            "example.com",
            "//example.com",
            "",
            "   ",
        ] {
            let e = validate_external_url(bad).unwrap_err();
            assert_eq!(e.code, ErrorCode::Validation, "{bad}");
            assert_eq!(e.field.as_deref(), Some("url"), "{bad}");
        }
    }

    #[test]
    fn malformed_web_and_mail_links_are_refused() {
        for bad in [
            "http:example.com",
            "http://",
            "https:///path",
            "https://user:pass@example.com",
            "https://good.com@evil.com/",
            "mailto:",
            "mailto:?subject=x",
            "mailto://x",
        ] {
            assert!(validate_external_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn control_characters_whitespace_and_length_are_refused() {
        for bad in [
            "https://exa\tmple.com",
            "https://example.com/\u{0}x",
            "https://example.com/a b",
            "https://example.com/\u{7f}x",
            "https://example.com/\r\nHost: evil",
            "https://example.com/\u{85}x",
        ] {
            assert!(validate_external_url(bad).is_err(), "{bad:?}");
        }
        let long = format!("https://example.com/{}", "a".repeat(URL_MAX_LEN));
        assert!(validate_external_url(&long).is_err());
        let exact = format!("https://e.com/{}", "a".repeat(URL_MAX_LEN - 14));
        assert_eq!(exact.chars().count(), URL_MAX_LEN);
        assert!(validate_external_url(&exact).is_ok());
    }
}
