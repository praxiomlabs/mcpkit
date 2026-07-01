//! Opaque cursor pagination for list results.
//!
//! MCP list operations (`tools/list`, `resources/list`, etc.) use an opaque
//! `cursor` and return an optional `nextCursor`. This module provides an
//! offset-based cursor codec and a helper to page a `Vec` of results.
//!
//! Cursors are versioned, base64url-encoded tokens; a client MUST treat them as
//! opaque. Because they are offset-based, if the underlying list changes between
//! page requests entries may be skipped or repeated — acceptable for the
//! mostly-static tool/prompt lists, and a documented limitation for dynamic
//! resource lists.

use crate::error::McpError;
use base64::Engine;
use serde::{Deserialize, Serialize};

const CURSOR_VERSION: u8 = 1;

#[derive(Serialize, Deserialize)]
struct CursorToken {
    v: u8,
    offset: usize,
}

/// Encode an offset as an opaque, versioned cursor token.
#[must_use]
pub fn encode_cursor(offset: usize) -> String {
    let json = serde_json::to_vec(&CursorToken {
        v: CURSOR_VERSION,
        offset,
    })
    .unwrap_or_default();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

/// Decode an opaque cursor into its offset.
///
/// # Errors
///
/// Returns `invalid_params` if the cursor is malformed or carries an unsupported
/// version.
pub fn decode_cursor(method: &str, cursor: &str) -> Result<usize, McpError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| McpError::invalid_params(method, "invalid cursor"))?;
    let token: CursorToken = serde_json::from_slice(&bytes)
        .map_err(|_| McpError::invalid_params(method, "invalid cursor"))?;
    if token.v != CURSOR_VERSION {
        return Err(McpError::invalid_params(
            method,
            "unsupported cursor version",
        ));
    }
    Ok(token.offset)
}

/// Paginate `items` given an inbound `cursor` and an optional `page_size`.
///
/// When `page_size` is `None` (or zero) pagination is disabled: all items from
/// the cursor offset are returned with no next cursor. Otherwise a single page
/// is returned, plus a `nextCursor` when more items remain.
///
/// # Errors
///
/// Returns `invalid_params` if `cursor` is malformed.
pub fn paginate<T>(
    items: Vec<T>,
    cursor: Option<&str>,
    page_size: Option<usize>,
    method: &str,
) -> Result<(Vec<T>, Option<String>), McpError> {
    let offset = match cursor {
        Some(c) => decode_cursor(method, c)?,
        None => 0,
    };

    let Some(page_size) = page_size.filter(|&n| n > 0) else {
        // Pagination disabled: return the remainder from the offset, no cursor.
        return Ok((items.into_iter().skip(offset).collect(), None));
    };

    if offset >= items.len() {
        return Ok((Vec::new(), None));
    }
    let end = (offset + page_size).min(items.len());
    let has_more = end < items.len();
    let page = items.into_iter().skip(offset).take(page_size).collect();
    let next = has_more.then(|| encode_cursor(end));
    Ok((page, next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrips_and_is_opaque() {
        let c = encode_cursor(100);
        assert!(
            !c.contains("100"),
            "cursor should be opaque, not a raw offset"
        );
        assert_eq!(decode_cursor("tools/list", &c).unwrap(), 100);
    }

    #[test]
    fn invalid_cursor_is_invalid_params() {
        assert!(decode_cursor("tools/list", "not-base64!!!").is_err());
        // Valid base64 but not a cursor token.
        let junk = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"{}");
        assert!(decode_cursor("tools/list", &junk).is_err());
    }

    #[test]
    fn disabled_pagination_returns_all() {
        let (page, next) = paginate(vec![1, 2, 3], None, None, "m").unwrap();
        assert_eq!(page, vec![1, 2, 3]);
        assert!(next.is_none());
    }

    #[test]
    fn pages_and_emits_next_cursor() {
        let items: Vec<u32> = (0..5).collect();
        // First page.
        let (page, next) = paginate(items.clone(), None, Some(2), "m").unwrap();
        assert_eq!(page, vec![0, 1]);
        let next = next.expect("more remain");
        // Second page via the cursor.
        let (page, next) = paginate(items.clone(), Some(&next), Some(2), "m").unwrap();
        assert_eq!(page, vec![2, 3]);
        let next = next.expect("more remain");
        // Final page: exactly one item, no further cursor.
        let (page, next) = paginate(items, Some(&next), Some(2), "m").unwrap();
        assert_eq!(page, vec![4]);
        assert!(next.is_none());
    }

    #[test]
    fn offset_at_or_past_end_is_empty() {
        let (page, next) = paginate(vec![1, 2], Some(&encode_cursor(2)), Some(10), "m").unwrap();
        assert!(page.is_empty());
        assert!(next.is_none());
    }
}
