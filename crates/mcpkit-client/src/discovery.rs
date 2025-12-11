//! Server discovery utilities.
//!
//! This module provides utilities for discovering MCP servers in the environment.
//! It supports:
//!
//! - Finding servers in standard locations
//! - Parsing server configuration files
//! - Spawning server processes

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A discovered MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredServer {
    /// Name of the server.
    pub name: String,
    /// How to connect to the server.
    pub transport: ServerTransport,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional icon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Environment variables to set when spawning.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

impl DiscoveredServer {
    /// Create a new discovered server with stdio transport.
    pub fn stdio(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: ServerTransport::Stdio {
                command: command.into(),
                args: Vec::new(),
            },
            description: None,
            icon: None,
            env: HashMap::new(),
        }
    }

    /// Create a new discovered server with HTTP transport.
    pub fn http(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: ServerTransport::Http { url: url.into() },
            description: None,
            icon: None,
            env: HashMap::new(),
        }
    }

    /// Set the description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an environment variable.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

/// Transport configuration for a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerTransport {
    /// Stdio transport (spawn a process).
    Stdio {
        /// Command to run.
        command: String,
        /// Command arguments.
        #[serde(default)]
        args: Vec<String>,
    },
    /// HTTP transport (connect to URL).
    Http {
        /// The server URL.
        url: String,
    },
    /// WebSocket transport.
    WebSocket {
        /// The WebSocket URL.
        url: String,
    },
}

/// Server discovery utility.
///
/// Discovers MCP servers from various sources:
///
/// - Configuration files in standard locations
/// - Environment variables
/// - Manual registration
pub struct ServerDiscovery {
    /// Known servers.
    servers: HashMap<String, DiscoveredServer>,
    /// Configuration file paths to check.
    config_paths: Vec<PathBuf>,
}

impl Default for ServerDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerDiscovery {
    /// Create a new server discovery instance.
    pub fn new() -> Self {
        let mut config_paths = Vec::new();

        // Add standard config locations
        if let Some(config_dir) = dirs_config_dir() {
            config_paths.push(config_dir.join("mcp").join("servers.json"));
        }
        if let Some(home) = dirs_home_dir() {
            config_paths.push(home.join(".mcp").join("servers.json"));
        }

        Self {
            servers: HashMap::new(),
            config_paths,
        }
    }

    /// Add a custom configuration file path.
    pub fn add_config_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config_paths.push(path.into());
        self
    }

    /// Register a server manually.
    pub fn register(mut self, server: DiscoveredServer) -> Self {
        self.servers.insert(server.name.clone(), server);
        self
    }

    /// Discover servers from all configured sources.
    ///
    /// # Errors
    ///
    /// Returns an error if reading configuration files fails.
    pub fn discover(&mut self) -> Result<(), DiscoveryError> {
        // Clone paths to avoid borrow conflict
        let paths: Vec<_> = self.config_paths.iter().cloned().collect();

        // Load from config files
        for path in paths {
            if path.exists() {
                self.load_config_file(&path)?;
            }
        }

        Ok(())
    }

    /// Get all discovered servers.
    pub fn servers(&self) -> impl Iterator<Item = &DiscoveredServer> {
        self.servers.values()
    }

    /// Get a server by name.
    pub fn get(&self, name: &str) -> Option<&DiscoveredServer> {
        self.servers.get(name)
    }

    /// Check if a server is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.servers.contains_key(name)
    }

    /// Load servers from a configuration file.
    fn load_config_file(&mut self, path: &Path) -> Result<(), DiscoveryError> {
        let contents = std::fs::read_to_string(path).map_err(|e| DiscoveryError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let config: ServerConfig =
            serde_json::from_str(&contents).map_err(|e| DiscoveryError::Parse {
                path: path.to_path_buf(),
                source: e,
            })?;

        for server in config.servers {
            self.servers.insert(server.name.clone(), server);
        }

        Ok(())
    }
}

/// Configuration file format.
#[derive(Debug, Deserialize)]
struct ServerConfig {
    servers: Vec<DiscoveredServer>,
}

/// Error type for server discovery.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// I/O error reading a file.
    #[error("Failed to read {path}: {source}")]
    Io {
        /// The file path.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// Parse error in configuration file.
    #[error("Failed to parse {path}: {source}")]
    Parse {
        /// The file path.
        path: PathBuf,
        /// The underlying error.
        source: serde_json::Error,
    },
}

// Platform-agnostic directory helpers
fn dirs_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs_home_dir().map(|h| h.join("Library").join("Application Support"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs_home_dir().map(|h| h.join(".config")))
    }
}

fn dirs_home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovered_server_stdio() {
        let server = DiscoveredServer::stdio("my-server", "my-server-bin")
            .description("A test server")
            .env("DEBUG", "1");

        assert_eq!(server.name, "my-server");
        assert!(matches!(server.transport, ServerTransport::Stdio { .. }));
        assert_eq!(server.env.get("DEBUG"), Some(&"1".to_string()));
    }

    #[test]
    fn test_discovered_server_http() {
        let server = DiscoveredServer::http("my-server", "http://localhost:8080");

        assert_eq!(server.name, "my-server");
        match server.transport {
            ServerTransport::Http { url } => assert_eq!(url, "http://localhost:8080"),
            _ => panic!("Expected HTTP transport"),
        }
    }

    #[test]
    fn test_server_discovery_register() {
        let discovery = ServerDiscovery::new()
            .register(DiscoveredServer::stdio("test", "test-bin"));

        assert!(discovery.contains("test"));
        assert!(!discovery.contains("unknown"));
    }

    #[test]
    fn test_transport_serialization() {
        let server = DiscoveredServer::stdio("test", "test-cmd");
        let json = serde_json::to_string(&server).unwrap();
        assert!(json.contains("\"type\":\"stdio\""));

        let server = DiscoveredServer::http("test", "http://localhost");
        let json = serde_json::to_string(&server).unwrap();
        assert!(json.contains("\"type\":\"http\""));
    }
}
