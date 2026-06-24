//! `Origin` header validation for HTTP MCP servers (DNS-rebinding protection).
//!
//! DNS rebinding lets a malicious web page bind its own domain to `127.0.0.1`
//! and then make same-origin requests to a local server. The defense is for the
//! server to validate the request's `Origin` header against an allow-list. Only
//! browsers send `Origin`, so non-browser clients (which cannot be used for DNS
//! rebinding) are unaffected.
//!
//! See: <https://modelcontextprotocol.io/specification/2025-11-25/basic/transports>

use super::server::OriginValidationMode;

/// Validates request `Origin` headers to protect an HTTP MCP server from
/// DNS-rebinding attacks.
///
/// The default ([`allow_list`](Self::allow_list)) is secure: only loopback
/// origins (`localhost`, `127.0.0.1`, `[::1]`) and any origins added with
/// [`allow`](Self::allow) are accepted, and requests with no `Origin` header are
/// allowed (they cannot come from a browser). An empty allow-list therefore
/// means "loopback only", **not** "allow all".
#[derive(Debug, Clone)]
pub struct OriginValidator {
    mode: OriginValidationMode,
    allowed_origins: Vec<String>,
}

impl Default for OriginValidator {
    fn default() -> Self {
        Self::allow_list()
    }
}

impl OriginValidator {
    /// The secure default: accept loopback origins plus any added with
    /// [`allow`](Self::allow); reject all other browser origins. Requests
    /// without an `Origin` header are allowed.
    #[must_use]
    pub const fn allow_list() -> Self {
        Self {
            mode: OriginValidationMode::AllowList,
            allowed_origins: Vec::new(),
        }
    }

    /// Disable origin validation: accept every origin.
    ///
    /// **Insecure** — this removes DNS-rebinding protection. Only use it behind
    /// other safeguards (mTLS, a trusted network, authenticated sessions).
    #[must_use]
    pub const fn allow_any() -> Self {
        Self {
            mode: OriginValidationMode::Disabled,
            allowed_origins: Vec::new(),
        }
    }

    /// Add an allowed origin, e.g. `https://app.example.com`. Matched exactly
    /// (scheme, host, and port).
    #[must_use]
    pub fn allow(mut self, origin: impl Into<String>) -> Self {
        self.allowed_origins.push(origin.into());
        self
    }

    /// Whether a request carrying this `Origin` header value should be allowed.
    #[must_use]
    pub fn is_allowed(&self, origin: Option<&str>) -> bool {
        match self.mode {
            OriginValidationMode::Disabled | OriginValidationMode::WarnAndAllow => true,
            OriginValidationMode::AllowList => match origin {
                // No Origin header: not a browser request, so not a DNS-rebinding
                // vector.
                None => true,
                Some(origin) => self.is_origin_listed(origin),
            },
            // Strict additionally requires that an `Origin` header be present.
            OriginValidationMode::Strict => origin.is_some_and(|o| self.is_origin_listed(o)),
        }
    }

    fn is_origin_listed(&self, origin: &str) -> bool {
        is_loopback_origin(origin) || self.allowed_origins.iter().any(|a| a == origin)
    }
}

/// Returns true if the origin's host is a loopback name/address
/// (`localhost`, `127.0.0.1`, or `[::1]`), regardless of scheme or port.
fn is_loopback_origin(origin: &str) -> bool {
    let Some((_scheme, rest)) = origin.split_once("://") else {
        return false;
    };
    // An Origin is scheme://host[:port]; guard against any stray path anyway.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = if let Some(after_bracket) = authority.strip_prefix('[') {
        // IPv6 literal, e.g. [::1]:8080
        after_bracket.split(']').next().unwrap_or(after_bracket)
    } else {
        // host[:port]
        authority.rsplit_once(':').map_or(authority, |(h, _)| h)
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_origins_are_allowed_by_default() {
        let v = OriginValidator::allow_list();
        assert!(v.is_allowed(Some("http://localhost")));
        assert!(v.is_allowed(Some("http://localhost:3000")));
        assert!(v.is_allowed(Some("http://127.0.0.1:8080")));
        assert!(v.is_allowed(Some("https://[::1]:9000")));
        assert!(v.is_allowed(Some("http://LocalHost:1234")));
    }

    #[test]
    fn external_origins_are_rejected_by_default() {
        let v = OriginValidator::allow_list();
        assert!(!v.is_allowed(Some("https://evil.example.com")));
        assert!(!v.is_allowed(Some("http://attacker.test")));
        // A host that merely contains "localhost" must not pass.
        assert!(!v.is_allowed(Some("http://localhost.evil.com")));
        assert!(!v.is_allowed(Some("http://127.0.0.1.evil.com")));
    }

    #[test]
    fn missing_origin_is_allowed_but_strict_rejects_it() {
        assert!(OriginValidator::allow_list().is_allowed(None));
        let strict = OriginValidator {
            mode: OriginValidationMode::Strict,
            allowed_origins: Vec::new(),
        };
        assert!(!strict.is_allowed(None));
        assert!(strict.is_allowed(Some("http://localhost")));
    }

    #[test]
    fn configured_origins_are_allowed_exactly() {
        let v = OriginValidator::allow_list().allow("https://app.example.com");
        assert!(v.is_allowed(Some("https://app.example.com")));
        assert!(!v.is_allowed(Some("https://app.example.com:8443")));
        assert!(!v.is_allowed(Some("http://app.example.com")));
        // Loopback still allowed alongside configured origins.
        assert!(v.is_allowed(Some("http://localhost:5173")));
    }

    #[test]
    fn allow_any_accepts_everything() {
        let v = OriginValidator::allow_any();
        assert!(v.is_allowed(Some("https://evil.example.com")));
        assert!(v.is_allowed(None));
    }
}
