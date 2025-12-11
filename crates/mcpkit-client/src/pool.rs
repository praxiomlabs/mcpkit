//! Client connection pooling.
//!
//! This module provides connection pooling for MCP clients,
//! allowing efficient reuse of connections to MCP servers.

use crate::builder::ClientBuilder;
use crate::client::Client;
use mcpkit_core::capability::{ClientCapabilities, ClientInfo};
use mcpkit_core::error::McpError;
use mcpkit_transport::Transport;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tracing::{debug, trace, warn};

// Pool is tokio-specific due to spawn and timeout requirements
use tokio::sync::{Mutex, Semaphore};

/// Configuration for a client connection pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections per server.
    pub max_connections: usize,
    /// Timeout for acquiring a connection.
    pub acquire_timeout: std::time::Duration,
    /// Whether to validate connections before use.
    pub validate_on_acquire: bool,
    /// Maximum idle time before a connection is closed.
    pub max_idle_time: std::time::Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            acquire_timeout: std::time::Duration::from_secs(30),
            validate_on_acquire: true,
            max_idle_time: std::time::Duration::from_secs(300),
        }
    }
}

impl PoolConfig {
    /// Create a new pool configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of connections.
    #[must_use]
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    /// Set the acquire timeout.
    #[must_use]
    pub fn acquire_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.acquire_timeout = timeout;
        self
    }

    /// Set whether to validate connections before use.
    #[must_use]
    pub fn validate_on_acquire(mut self, validate: bool) -> Self {
        self.validate_on_acquire = validate;
        self
    }

    /// Set the maximum idle time.
    #[must_use]
    pub fn max_idle_time(mut self, time: std::time::Duration) -> Self {
        self.max_idle_time = time;
        self
    }
}

/// A pooled client connection.
///
/// When dropped, the connection is returned to the pool.
pub struct PooledClient<T: Transport + 'static> {
    client: Option<Client<T>>,
    pool: Arc<ClientPoolInner<T>>,
    key: String,
}

impl<T: Transport + 'static> PooledClient<T> {
    /// Get a reference to the underlying client.
    pub fn client(&self) -> &Client<T> {
        self.client.as_ref().expect("Client already dropped")
    }

    /// Get a mutable reference to the underlying client.
    pub fn client_mut(&mut self) -> &mut Client<T> {
        self.client.as_mut().expect("Client already dropped")
    }
}

impl<T: Transport + 'static> std::ops::Deref for PooledClient<T> {
    type Target = Client<T>;

    fn deref(&self) -> &Self::Target {
        self.client()
    }
}

impl<T: Transport + 'static> std::ops::DerefMut for PooledClient<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.client_mut()
    }
}

impl<T: Transport + 'static> Drop for PooledClient<T> {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            // Return the connection to the pool
            let pool = Arc::clone(&self.pool);
            let key = self.key.clone();
            tokio::spawn(async move {
                pool.return_connection(key, client).await;
            });
        }
    }
}

/// Internal pool state.
struct ClientPoolInner<T: Transport> {
    /// Configuration.
    config: PoolConfig,
    /// Available connections by server key.
    connections: Mutex<HashMap<String, Vec<PooledEntry<T>>>>,
    /// Semaphore for limiting concurrent connections.
    semaphores: Mutex<HashMap<String, Arc<Semaphore>>>,
    /// Client info to use for new connections.
    client_info: ClientInfo,
    /// Client capabilities.
    client_caps: ClientCapabilities,
}

/// An entry in the pool.
struct PooledEntry<T: Transport> {
    client: Client<T>,
    last_used: std::time::Instant,
}

impl<T: Transport> ClientPoolInner<T> {
    /// Return a connection to the pool.
    async fn return_connection(&self, key: String, client: Client<T>) {
        trace!(%key, "Returning connection to pool");

        let entry = PooledEntry {
            client,
            last_used: std::time::Instant::now(),
        };

        let mut connections = self.connections.lock().await;
        connections
            .entry(key)
            .or_insert_with(Vec::new)
            .push(entry);
    }

    /// Get a semaphore for rate limiting connections to a server.
    async fn get_semaphore(&self, key: &str) -> Arc<Semaphore> {
        let mut semaphores = self.semaphores.lock().await;
        semaphores
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.config.max_connections)))
            .clone()
    }
}

/// A pool of client connections.
///
/// The pool manages connections to multiple MCP servers, reusing
/// existing connections when possible and creating new ones as needed.
///
/// # Example
///
/// ```no_run
/// use mcpkit_client::{ClientPool, ClientPoolBuilder};
/// use mcpkit_transport::SpawnedTransport;
/// use mcpkit_core::error::McpError;
///
/// # async fn example() -> Result<(), McpError> {
/// let pool = ClientPool::<SpawnedTransport>::builder()
///     .client_info("my-client", "1.0.0")
///     .max_connections(5)
///     .build();
///
/// let client = pool.acquire("server-key", || async {
///     // Create a new connection to a server
///     // TransportError converts to McpError automatically
///     Ok::<_, McpError>(
///         SpawnedTransport::spawn("my-server", &[] as &[&str]).await?
///     )
/// }).await?;
///
/// // Use the client
/// let tools = client.list_tools().await?;
///
/// // Client is returned to pool when dropped
/// # Ok(())
/// # }
/// ```
pub struct ClientPool<T: Transport> {
    inner: Arc<ClientPoolInner<T>>,
}

impl<T: Transport + 'static> ClientPool<T> {
    /// Create a new pool builder.
    pub fn builder() -> ClientPoolBuilder {
        ClientPoolBuilder::new()
    }

    /// Create a new pool with default configuration.
    pub fn new(client_info: ClientInfo, client_caps: ClientCapabilities) -> Self {
        Self::with_config(client_info, client_caps, PoolConfig::default())
    }

    /// Create a new pool with custom configuration.
    pub fn with_config(
        client_info: ClientInfo,
        client_caps: ClientCapabilities,
        config: PoolConfig,
    ) -> Self {
        Self {
            inner: Arc::new(ClientPoolInner {
                config,
                connections: Mutex::new(HashMap::new()),
                semaphores: Mutex::new(HashMap::new()),
                client_info,
                client_caps,
            }),
        }
    }

    /// Acquire a connection from the pool.
    ///
    /// If a cached connection is available, it is returned. Otherwise,
    /// the `connect` function is called to create a new connection.
    ///
    /// # Arguments
    ///
    /// * `key` - A unique key identifying the server
    /// * `connect` - A function that creates a new transport connection
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be acquired.
    pub async fn acquire<F, Fut>(
        &self,
        key: impl Into<String>,
        connect: F,
    ) -> Result<PooledClient<T>, McpError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, McpError>>,
    {
        let key = key.into();
        debug!(%key, "Acquiring connection from pool");

        // Get the semaphore for rate limiting
        let semaphore = self.inner.get_semaphore(&key).await;

        // Acquire a permit (with timeout)
        let _permit = tokio::time::timeout(
            self.inner.config.acquire_timeout,
            semaphore.acquire_owned(),
        )
        .await
        .map_err(|_| McpError::Internal {
            message: format!("Timeout acquiring connection for {key}"),
            source: None,
        })?
        .map_err(|_| McpError::Internal {
            message: "Pool semaphore closed".to_string(),
            source: None,
        })?;

        // Try to get an existing connection
        {
            let mut connections = self.inner.connections.lock().await;
            if let Some(entries) = connections.get_mut(&key) {
                // Remove stale connections
                let max_idle = self.inner.config.max_idle_time;
                entries.retain(|e| e.last_used.elapsed() < max_idle);

                // Get a connection if available
                if let Some(entry) = entries.pop() {
                    trace!(%key, "Reusing existing connection");

                    // Optionally validate the connection
                    if self.inner.config.validate_on_acquire {
                        // Try to ping
                        if entry.client.ping().await.is_ok() {
                            return Ok(PooledClient {
                                client: Some(entry.client),
                                pool: Arc::clone(&self.inner),
                                key,
                            });
                        }
                        warn!(%key, "Cached connection failed validation");
                    } else {
                        return Ok(PooledClient {
                            client: Some(entry.client),
                            pool: Arc::clone(&self.inner),
                            key,
                        });
                    }
                }
            }
        }

        // Create a new connection
        debug!(%key, "Creating new connection");
        let transport = connect().await?;

        let client = ClientBuilder::new()
            .name(self.inner.client_info.name.clone())
            .version(self.inner.client_info.version.clone())
            .capabilities(self.inner.client_caps.clone())
            .build(transport)
            .await?;

        Ok(PooledClient {
            client: Some(client),
            pool: Arc::clone(&self.inner),
            key,
        })
    }

    /// Clear all cached connections.
    pub async fn clear(&self) {
        let mut connections = self.inner.connections.lock().await;
        connections.clear();
        debug!("Cleared all pooled connections");
    }

    /// Clear cached connections for a specific server.
    pub async fn clear_server(&self, key: &str) {
        let mut connections = self.inner.connections.lock().await;
        connections.remove(key);
        debug!(%key, "Cleared pooled connections for server");
    }

    /// Get statistics about the pool.
    pub async fn stats(&self) -> PoolStats {
        let connections = self.inner.connections.lock().await;
        let mut total = 0;
        let mut per_server = HashMap::new();

        for (key, entries) in connections.iter() {
            let count = entries.len();
            total += count;
            per_server.insert(key.clone(), count);
        }

        PoolStats {
            total_connections: total,
            connections_per_server: per_server,
            max_connections: self.inner.config.max_connections,
        }
    }
}

impl<T: Transport + 'static> Clone for ClientPool<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Statistics about a connection pool.
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Total number of cached connections.
    pub total_connections: usize,
    /// Number of connections per server.
    pub connections_per_server: HashMap<String, usize>,
    /// Maximum connections per server.
    pub max_connections: usize,
}

/// Builder for creating a client pool.
pub struct ClientPoolBuilder {
    config: PoolConfig,
    client_info: Option<ClientInfo>,
    client_caps: ClientCapabilities,
}

impl ClientPoolBuilder {
    /// Create a new pool builder.
    pub fn new() -> Self {
        Self {
            config: PoolConfig::default(),
            client_info: None,
            client_caps: ClientCapabilities::default(),
        }
    }

    /// Set the client info.
    pub fn client_info(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.client_info = Some(ClientInfo {
            name: name.into(),
            version: version.into(),
        });
        self
    }

    /// Set the client capabilities.
    pub fn capabilities(mut self, caps: ClientCapabilities) -> Self {
        self.client_caps = caps;
        self
    }

    /// Set the maximum number of connections per server.
    pub fn max_connections(mut self, max: usize) -> Self {
        self.config.max_connections = max;
        self
    }

    /// Set the acquire timeout.
    pub fn acquire_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.acquire_timeout = timeout;
        self
    }

    /// Set whether to validate connections on acquire.
    pub fn validate_on_acquire(mut self, validate: bool) -> Self {
        self.config.validate_on_acquire = validate;
        self
    }

    /// Set the maximum idle time.
    pub fn max_idle_time(mut self, time: std::time::Duration) -> Self {
        self.config.max_idle_time = time;
        self
    }

    /// Build the pool.
    ///
    /// # Panics
    ///
    /// Panics if client_info was not set.
    pub fn build<T: Transport + 'static>(self) -> ClientPool<T> {
        let client_info = self
            .client_info
            .expect("client_info must be set before building pool");

        ClientPool::with_config(client_info, self.client_caps, self.config)
    }
}

impl Default for ClientPoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config() {
        let config = PoolConfig::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .validate_on_acquire(false)
            .max_idle_time(std::time::Duration::from_secs(60));

        assert_eq!(config.max_connections, 5);
        assert_eq!(config.acquire_timeout.as_secs(), 10);
        assert!(!config.validate_on_acquire);
        assert_eq!(config.max_idle_time.as_secs(), 60);
    }

    #[test]
    fn test_pool_builder() {
        let builder = ClientPoolBuilder::new()
            .client_info("test-client", "1.0.0")
            .max_connections(10)
            .validate_on_acquire(true);

        assert_eq!(builder.config.max_connections, 10);
        assert!(builder.config.validate_on_acquire);
    }
}
