use axum::{
    extract::Request,
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use subtle::ConstantTimeEq;

/// Bearer token auth middleware. Fails CLOSED: if no token is configured
/// and insecure mode is not enabled, all requests are rejected.
///
/// Token comparison uses `subtle::ConstantTimeEq` to defeat remote
/// timing attacks (see security-threats.md T3).
pub async fn bearer_auth(
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let insecure = std::env::var("NLR_INSECURE_NO_AUTH").is_ok();

    let expected = match std::env::var("NLR_API_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ if insecure => return Ok(next.run(request).await),
        _ => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "server_misconfigured",
                    "message": "No API token configured. Start with --token <value> or --insecure-no-auth for local dev.",
                    "status": 500
                })),
            ).into_response());
        }
    };

    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let ok = auth_header
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|t| ct_eq_str(t, &expected))
        .unwrap_or(false);

    if ok {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED.into_response())
    }
}

/// Constant-time string equality.
///
/// Both sides are hashed with SHA-256 first so differing-length inputs go
/// through the same amount of work and `ct_eq` always compares fixed-size
/// digests. This eliminates any observable divergence between the
/// "wrong length" and "wrong bytes" paths. The extra `len_eq` check in
/// constant time guards against the (astronomically unlikely) digest
/// collision case.
fn ct_eq_str(a: &str, b: &str) -> bool {
    use sha2::{Digest, Sha256};
    let ha = Sha256::digest(a.as_bytes());
    let hb = Sha256::digest(b.as_bytes());
    let digest_eq: bool = ha.ct_eq(&hb).into();
    let len_eq = (a.len() as u64 ^ b.len() as u64) == 0;
    digest_eq && len_eq
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_equal_tokens_succeed() {
        let tok = "abc123XYZ-secret-token-value";
        assert!(ct_eq_str(tok, tok));
    }

    #[test]
    fn differing_tokens_fail() {
        assert!(!ct_eq_str("abc123", "abc124"));
        assert!(!ct_eq_str("short", "muchlongerwrongtoken"));
        assert!(!ct_eq_str("", "nonempty"));
        assert!(!ct_eq_str("nonempty", ""));
    }

    #[test]
    fn empty_equals_empty() {
        assert!(ct_eq_str("", ""));
    }

    #[test]
    fn different_length_mismatches_do_not_short_circuit() {
        // Property: the function goes through SHA-256 on both inputs regardless of
        // length, so the amount of work is bounded by input size rather than
        // branching on a short common prefix. This is a type-system / structural
        // guarantee, not a real timing test (see the physical-review section of
        // the checklist for the `hey` / `ab` live test).
        let expected = "a_real_32_byte_bearer_token_xxxx";
        let wrong_short = "a";
        let wrong_same_len = "b_wrong_32_byte_bearer_token_xxx";
        let wrong_long =
            "a_real_32_byte_bearer_token_xxxx_plus_extra_suffix_that_should_also_fail";

        assert!(!ct_eq_str(wrong_short, expected));
        assert!(!ct_eq_str(wrong_same_len, expected));
        assert!(!ct_eq_str(wrong_long, expected));
        assert!(ct_eq_str(expected, expected));
    }

    #[test]
    fn bearer_prefix_handling() {
        // Non-Bearer schemes must fail outright (no Bearer prefix to strip).
        let hdr_basic = "Basic dXNlcjpwYXNz";
        assert!(hdr_basic.strip_prefix("Bearer ").is_none());
        // With Bearer prefix the comparison reaches ct_eq and matches only
        // the exact token.
        let expected = "tok";
        let hdr_bearer = "Bearer tok";
        let stripped = hdr_bearer.strip_prefix("Bearer ").unwrap();
        assert!(ct_eq_str(stripped, expected));
        // A longer bearer token does not accidentally match a common prefix.
        let hdr_bearer_prefixed = "Bearer toktok";
        let stripped = hdr_bearer_prefixed.strip_prefix("Bearer ").unwrap();
        assert!(!ct_eq_str(stripped, expected));
    }
}
