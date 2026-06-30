//! Verified user identity and session-to-user binding.
//!
//! The MCP security best practices require that per-user state is not associated
//! with a session id alone — user identity should derive from the validated
//! access token (its `sub`, scoped by `iss`). [`VerifiedUser`] is that identity,
//! and [`check_session_binding`] enforces that a session bound to a user is only
//! ever used by that same user.

use serde::{Deserialize, Serialize};

/// A user identity verified from an access token.
///
/// Identity is the `(issuer, subject)` pair: a `subject` (`sub`) is only
/// globally meaningful within its `issuer` (`iss`). `audience` is recorded for
/// context but is deliberately *not* part of identity equality — validating the
/// audience is a token-validation concern (is this token meant for this
/// resource?), and a returning user's token may legitimately be re-issued with a
/// different audience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedUser {
    /// The token subject (`sub`).
    pub subject: String,
    /// The token issuer (`iss`), if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// The token audience(s) (`aud`). Context only; not used for binding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audience: Vec<String>,
}

impl VerifiedUser {
    /// Create a verified user from a subject.
    #[must_use]
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            issuer: None,
            audience: Vec::new(),
        }
    }

    /// Set the issuer (`iss`).
    #[must_use]
    pub fn issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Set the audience(s) (`aud`).
    #[must_use]
    pub fn audience<I, S>(mut self, audience: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.audience = audience.into_iter().map(Into::into).collect();
        self
    }

    /// Whether two identities are the same user — equal `(issuer, subject)`.
    ///
    /// Audience is intentionally not compared.
    #[must_use]
    pub fn is_same_user(&self, other: &Self) -> bool {
        self.subject == other.subject && self.issuer == other.issuer
    }

    /// Build a verified user from validated JWT claims, or `None` if the token
    /// has no `sub`.
    ///
    /// The caller is responsible for having *validated* the token first; this
    /// only projects the claims into an identity.
    #[cfg(feature = "jwt")]
    #[must_use]
    pub fn from_claims(claims: &crate::auth::jwt::TokenClaims) -> Option<Self> {
        use crate::auth::jwt::Audience;
        let subject = claims.sub.clone()?;
        let audience = match &claims.aud {
            Some(Audience::Single(a)) => vec![a.clone()],
            Some(Audience::Multiple(v)) => v.clone(),
            None => Vec::new(),
        };
        Some(Self {
            subject,
            issuer: claims.iss.clone(),
            audience,
        })
    }
}

/// Why a request was refused against a session's bound identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionBindingError {
    /// The session is bound to a user, but the request presented no identity.
    IdentityRequired,
    /// The session is bound to a different user than the request presented.
    IdentityMismatch,
    /// The session is anonymous, but the request presented a verified identity.
    /// A user-bound session must be created up front rather than silently
    /// upgraded from an anonymous one.
    UnexpectedIdentity,
}

impl std::fmt::Display for SessionBindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::IdentityRequired => "session requires a verified identity",
            Self::IdentityMismatch => "session is bound to a different user",
            Self::UnexpectedIdentity => "anonymous session cannot be used with a verified identity",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for SessionBindingError {}

/// Enforce the session-to-user binding rule for a request.
///
/// - anonymous session + anonymous request → `Ok`
/// - user-bound session + same user → `Ok`
/// - user-bound session + no identity → [`SessionBindingError::IdentityRequired`]
/// - user-bound session + different user → [`SessionBindingError::IdentityMismatch`]
/// - anonymous session + verified identity → [`SessionBindingError::UnexpectedIdentity`]
///   (no silent upgrade)
pub fn check_session_binding(
    bound: Option<&VerifiedUser>,
    presenting: Option<&VerifiedUser>,
) -> Result<(), SessionBindingError> {
    match (bound, presenting) {
        (None, None) => Ok(()),
        (None, Some(_)) => Err(SessionBindingError::UnexpectedIdentity),
        (Some(_), None) => Err(SessionBindingError::IdentityRequired),
        (Some(b), Some(p)) if b.is_same_user(p) => Ok(()),
        (Some(_), Some(_)) => Err(SessionBindingError::IdentityMismatch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_user_compares_issuer_and_subject_not_audience() {
        let a = VerifiedUser::new("alice")
            .issuer("https://idp")
            .audience(["res-1"]);
        let b = VerifiedUser::new("alice")
            .issuer("https://idp")
            .audience(["res-2"]);
        assert!(a.is_same_user(&b), "audience must not affect identity");

        let c = VerifiedUser::new("alice").issuer("https://other");
        assert!(!a.is_same_user(&c), "different issuer is a different user");

        let d = VerifiedUser::new("bob").issuer("https://idp");
        assert!(!a.is_same_user(&d), "different subject is a different user");
    }

    #[test]
    fn binding_rules() {
        let alice = VerifiedUser::new("alice").issuer("https://idp");
        let bob = VerifiedUser::new("bob").issuer("https://idp");

        // anonymous session + anonymous request
        assert!(check_session_binding(None, None).is_ok());
        // bound + same user
        assert!(check_session_binding(Some(&alice), Some(&alice)).is_ok());
        // bound + missing identity
        assert_eq!(
            check_session_binding(Some(&alice), None),
            Err(SessionBindingError::IdentityRequired)
        );
        // bound + different user
        assert_eq!(
            check_session_binding(Some(&alice), Some(&bob)),
            Err(SessionBindingError::IdentityMismatch)
        );
        // anonymous session + verified identity (no silent upgrade)
        assert_eq!(
            check_session_binding(None, Some(&alice)),
            Err(SessionBindingError::UnexpectedIdentity)
        );
    }
}
