//! Transport error types and context.
//!
//! This module provides classification and contextual information
//! for transport-level errors.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Classification of transport errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportErrorKind {
    /// Connection could not be established.
    ConnectionFailed,
    /// Connection was closed unexpectedly.
    ConnectionClosed,
    /// Read operation failed.
    ReadFailed,
    /// Write operation failed.
    WriteFailed,
    /// TLS/SSL error occurred.
    TlsError,
    /// DNS resolution failed.
    DnsResolutionFailed,
    /// Operation timed out.
    Timeout,
    /// Message format was invalid.
    InvalidMessage,
    /// Protocol violation detected.
    ProtocolViolation,
    /// Resources exhausted (e.g., too many connections).
    ResourceExhausted,
    /// Rate limit exceeded.
    RateLimited,
}

impl fmt::Display for TransportErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectionFailed => write!(f, "connection failed"),
            Self::ConnectionClosed => write!(f, "connection closed"),
            Self::ReadFailed => write!(f, "read failed"),
            Self::WriteFailed => write!(f, "write failed"),
            Self::TlsError => write!(f, "TLS error"),
            Self::DnsResolutionFailed => write!(f, "DNS resolution failed"),
            Self::Timeout => write!(f, "timeout"),
            Self::InvalidMessage => write!(f, "invalid message"),
            Self::ProtocolViolation => write!(f, "protocol violation"),
            Self::ResourceExhausted => write!(f, "resource exhausted"),
            Self::RateLimited => write!(f, "rate limited"),
        }
    }
}

/// Additional context for transport errors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportContext {
    /// Transport type (stdio, http, websocket, unix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    /// Remote endpoint address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,
    /// Local endpoint address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_addr: Option<String>,
    /// Bytes sent before error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_sent: Option<u64>,
    /// Bytes received before error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_received: Option<u64>,
    /// Connection duration before error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_duration_ms: Option<u64>,
}

impl TransportContext {
    /// Create a new transport context for a specific transport type.
    #[must_use]
    pub fn new(transport_type: impl Into<String>) -> Self {
        Self {
            transport_type: Some(transport_type.into()),
            ..Default::default()
        }
    }

    /// Set the remote address.
    #[must_use]
    pub fn with_remote_addr(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = Some(addr.into());
        self
    }

    /// Set the local address.
    #[must_use]
    pub fn with_local_addr(mut self, addr: impl Into<String>) -> Self {
        self.local_addr = Some(addr.into());
        self
    }
}
