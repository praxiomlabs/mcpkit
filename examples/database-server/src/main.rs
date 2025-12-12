//! Database Server Example
//!
//! A comprehensive MCP server that provides database access through tools,
//! exposes table schemas as resources, and offers query generation prompts.
//!
//! This example demonstrates:
//! - Complex tool implementations with validation
//! - Dynamic resources based on database schema
//! - Prompt templates for SQL generation
//! - Error handling and recovery
//! - Structured JSON responses
//!
//! ## Running
//!
//! ```bash
//! # Create a sample database
//! sqlite3 test.db "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT);"
//! sqlite3 test.db "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com');"
//!
//! # Run the server
//! DATABASE_URL=sqlite:test.db cargo run -p database-server-example
//! ```

use mcpkit::prelude::*;
use mcpkit_server::Context;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Simulated database connection
/// In a real implementation, this would use sqlx or another database driver
struct Database {
    tables: RwLock<Vec<TableInfo>>,
    data: RwLock<Vec<serde_json::Value>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct TableInfo {
    name: String,
    columns: Vec<ColumnInfo>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ColumnInfo {
    name: String,
    data_type: String,
    nullable: bool,
}

impl Database {
    fn new() -> Self {
        // Initialize with sample schema
        let tables = vec![
            TableInfo {
                name: "users".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        data_type: "TEXT".to_string(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "email".to_string(),
                        data_type: "TEXT".to_string(),
                        nullable: true,
                    },
                    ColumnInfo {
                        name: "created_at".to_string(),
                        data_type: "TIMESTAMP".to_string(),
                        nullable: false,
                    },
                ],
            },
            TableInfo {
                name: "orders".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "user_id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "total".to_string(),
                        data_type: "DECIMAL".to_string(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "status".to_string(),
                        data_type: "TEXT".to_string(),
                        nullable: false,
                    },
                ],
            },
        ];

        let data = vec![
            serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com", "created_at": "2024-01-01T00:00:00Z"}),
            serde_json::json!({"id": 2, "name": "Bob", "email": "bob@example.com", "created_at": "2024-01-02T00:00:00Z"}),
            serde_json::json!({"id": 3, "name": "Charlie", "email": null, "created_at": "2024-01-03T00:00:00Z"}),
        ];

        Self {
            tables: RwLock::new(tables),
            data: RwLock::new(data),
        }
    }
}

/// The database MCP server
struct DatabaseServer {
    db: Arc<Database>,
}

impl DatabaseServer {
    fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

#[mcp_server(
    name = "database-server",
    version = "1.0.0",
    instructions = "This server provides database access. Use the query tool to run SELECT queries, \
                   and explore table schemas through resources."
)]
impl DatabaseServer {
    // =========================================================================
    // TOOLS
    // =========================================================================

    /// Execute a SELECT query against the database.
    ///
    /// Only SELECT queries are allowed for safety.
    #[tool(
        description = "Execute a read-only SQL SELECT query against the database",
        read_only = true
    )]
    async fn query(&self, sql: String, limit: Option<u32>) -> Result<ToolOutput, McpError> {
        // Validate query is SELECT only
        let sql_upper = sql.trim().to_uppercase();
        if !sql_upper.starts_with("SELECT") {
            return Err(McpError::invalid_params(
                "query",
                "Only SELECT queries are allowed. Use query_write for modifications.",
            ));
        }

        // Apply limit
        let limit = limit.unwrap_or(100).min(1000);

        // Simulate query execution
        let data = self.db.data.read().await;
        let results: Vec<_> = data.iter().take(limit as usize).cloned().collect();

        let result = serde_json::json!({
            "success": true,
            "row_count": results.len(),
            "rows": results,
            "query": sql,
        });
        ToolOutput::json(&result).map_err(|e| McpError::internal(e.to_string()))
    }

    /// List all tables in the database.
    #[tool(description = "List all tables in the database", read_only = true)]
    async fn list_tables(&self) -> ToolOutput {
        let tables = self.db.tables.read().await;
        let table_names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();

        let result = serde_json::json!({
            "tables": table_names,
            "count": table_names.len(),
        });
        ToolOutput::text(serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string()))
    }

    /// Get the schema for a specific table.
    #[tool(
        description = "Get detailed schema information for a table",
        read_only = true
    )]
    async fn describe_table(&self, table_name: String) -> Result<ToolOutput, McpError> {
        let tables = self.db.tables.read().await;

        let table = tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(&table_name))
            .ok_or_else(|| {
                McpError::invalid_params(
                    "describe_table",
                    format!("Table '{}' not found", table_name),
                )
            })?;

        let result = serde_json::json!({
            "table": table.name,
            "columns": table.columns,
            "column_count": table.columns.len(),
        });
        ToolOutput::json(&result).map_err(|e| McpError::internal(e.to_string()))
    }

    /// Insert a new record into a table.
    #[tool(
        description = "Insert a new record into a table. Requires table name and record data as JSON.",
        destructive = true
    )]
    async fn insert_record(
        &self,
        table_name: String,
        record: serde_json::Value,
    ) -> Result<ToolOutput, McpError> {
        // Validate table exists
        let tables = self.db.tables.read().await;
        if !tables
            .iter()
            .any(|t| t.name.eq_ignore_ascii_case(&table_name))
        {
            return Err(McpError::invalid_params(
                "insert_record",
                format!("Table '{}' not found", table_name),
            ));
        }
        drop(tables);

        // Validate record is an object
        if !record.is_object() {
            return Err(McpError::invalid_params(
                "insert_record",
                "Record must be a JSON object",
            ));
        }

        // Simulate insert
        let mut data = self.db.data.write().await;
        let new_id = data.len() + 1;
        let mut new_record = record;
        if let Some(obj) = new_record.as_object_mut() {
            obj.insert("id".to_string(), serde_json::json!(new_id));
        }
        data.push(new_record.clone());

        let result = serde_json::json!({
            "success": true,
            "message": format!("Record inserted into {}", table_name),
            "id": new_id,
            "record": new_record,
        });
        ToolOutput::json(&result).map_err(|e| McpError::internal(e.to_string()))
    }

    // =========================================================================
    // RESOURCES
    // =========================================================================

    /// Get the complete database schema.
    #[resource(
        uri_pattern = "db://schema",
        name = "Database Schema",
        description = "Complete schema of all tables in the database",
        mime_type = "application/json"
    )]
    async fn get_schema(&self, _uri: &str) -> ResourceContents {
        let tables = self.db.tables.read().await;
        let schema = serde_json::to_string_pretty(&*tables).unwrap_or_default();
        ResourceContents::text("db://schema", &schema)
    }

    /// Get schema for a specific table.
    #[resource(
        uri_pattern = "db://tables/{table_name}",
        name = "Table Schema",
        description = "Schema for a specific table",
        mime_type = "application/json"
    )]
    async fn get_table_schema(&self, uri: &str) -> Result<ResourceContents, McpError> {
        let table_name = uri
            .strip_prefix("db://tables/")
            .ok_or_else(|| McpError::invalid_request("Invalid URI format"))?;

        let tables = self.db.tables.read().await;
        let table = tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(table_name))
            .ok_or_else(|| McpError::resource_not_found(uri))?;

        let schema = serde_json::to_string_pretty(table).unwrap_or_default();
        Ok(ResourceContents::text(uri, &schema))
    }

    /// Sample data from a table.
    #[resource(
        uri_pattern = "db://tables/{table_name}/sample",
        name = "Table Sample Data",
        description = "Sample rows from a table (first 10 rows)",
        mime_type = "application/json"
    )]
    async fn get_table_sample(&self, uri: &str) -> Result<ResourceContents, McpError> {
        let table_name = uri
            .strip_prefix("db://tables/")
            .and_then(|s| s.strip_suffix("/sample"))
            .ok_or_else(|| McpError::invalid_request("Invalid URI format"))?;

        // Validate table exists
        let tables = self.db.tables.read().await;
        if !tables
            .iter()
            .any(|t| t.name.eq_ignore_ascii_case(table_name))
        {
            return Err(McpError::resource_not_found(uri));
        }
        drop(tables);

        // Get sample data
        let data = self.db.data.read().await;
        let sample: Vec<_> = data.iter().take(10).cloned().collect();

        let json = serde_json::to_string_pretty(&sample).unwrap_or_default();
        Ok(ResourceContents::text(uri, &json))
    }

    // =========================================================================
    // PROMPTS
    // =========================================================================

    /// Generate a SELECT query for a table.
    #[prompt(description = "Generate a SELECT query based on requirements")]
    async fn generate_select(
        &self,
        table_name: String,
        columns: Option<String>,
        conditions: Option<String>,
    ) -> GetPromptResult {
        let cols = columns.unwrap_or_else(|| "*".to_string());
        let conditions_str = conditions.as_deref().unwrap_or("none");
        let where_clause = conditions
            .as_ref()
            .map(|c| format!("\nWHERE {}", c))
            .unwrap_or_default();

        GetPromptResult {
            description: Some(format!("SELECT query for {}", table_name)),
            messages: vec![
                PromptMessage::user(format!(
                    "You are a SQL expert. Generate efficient, safe SQL queries. \
                     Always use parameterized queries for user input.\n\n\
                     Generate a SELECT query for the '{}' table.\n\n\
                     Columns: {}\n\
                     Conditions: {}\n\n\
                     Please provide the SQL query and explain any optimizations.",
                    table_name, cols, conditions_str
                )),
                PromptMessage::assistant(format!(
                    "Here's the SQL query:\n\n```sql\nSELECT {}\nFROM {}{}\n```\n\n\
                     Would you like me to add ordering, grouping, or joins?",
                    cols, table_name, where_clause
                )),
            ],
        }
    }

    /// Help design a database schema.
    #[prompt(description = "Design a database schema for a given use case")]
    async fn design_schema(&self, use_case: String, entities: Option<String>) -> GetPromptResult {
        let entities_text = entities
            .map(|e| format!("\n\nEntities to include: {}", e))
            .unwrap_or_default();

        GetPromptResult {
            description: Some("Database schema design assistance".to_string()),
            messages: vec![PromptMessage::user(format!(
                "You are a database architect. Design normalized, efficient schemas. \
                 Follow best practices: use appropriate data types, add indexes, \
                 define foreign keys, and consider scalability.\n\n\
                 Please design a database schema for the following use case:\n\n\
                 {}{}\n\n\
                 Include:\n\
                 1. Table definitions with columns and types\n\
                 2. Primary and foreign keys\n\
                 3. Suggested indexes\n\
                 4. Any normalization recommendations",
                use_case, entities_text
            ))],
        }
    }

    /// Optimize a slow query.
    #[prompt(description = "Analyze and optimize a slow SQL query")]
    async fn optimize_query(
        &self,
        slow_query: String,
        execution_time: Option<String>,
    ) -> GetPromptResult {
        let time_info = execution_time
            .map(|t| format!(" (current execution time: {})", t))
            .unwrap_or_default();

        GetPromptResult {
            description: Some("Query optimization analysis".to_string()),
            messages: vec![PromptMessage::user(format!(
                "You are a database performance expert. Analyze queries for optimization \
                 opportunities. Consider indexes, query structure, joins, and execution plans.\n\n\
                 Please analyze and optimize this slow query{}:\n\n```sql\n{}\n```\n\n\
                 Provide:\n\
                 1. Identified performance issues\n\
                 2. Optimized query version\n\
                 3. Suggested indexes\n\
                 4. Estimated improvement",
                time_info, slow_query
            ))],
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .init();

    tracing::info!("Starting database MCP server...");

    // Create database
    let db = Arc::new(Database::new());

    // Create server
    let server = DatabaseServer::new(db);

    // Display available capabilities
    let info = <DatabaseServer as ServerHandler>::server_info(&server);
    tracing::info!("Server: {} v{}", info.name, info.version);

    // In a real application, we would serve over a transport
    // For this example, we'll just demonstrate the API
    println!("Database MCP Server initialized!");
    println!();
    println!("Available tools:");
    println!("  - query: Execute SELECT queries");
    println!("  - list_tables: List all tables");
    println!("  - describe_table: Get table schema");
    println!("  - insert_record: Insert new records");
    println!();
    println!("Available resources:");
    println!("  - db://schema: Complete database schema");
    println!("  - db://tables/{{name}}: Schema for a specific table");
    println!("  - db://tables/{{name}}/sample: Sample data from a table");
    println!();
    println!("Available prompts:");
    println!("  - generate_select: Generate SELECT queries");
    println!("  - design_schema: Help design database schemas");
    println!("  - optimize_query: Optimize slow queries");

    // Demo: Execute a query
    println!();
    println!("=== Demo: Executing query ===");

    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_server::context::NoOpPeer;

    let request_id = RequestId::Number(1);
    let client_caps = ClientCapabilities::default();
    let server_caps = ServerCapabilities::default();
    let peer = NoOpPeer;
    let _ctx = Context::new(&request_id, None, &client_caps, &server_caps, &peer);

    let result = server
        .query("SELECT * FROM users".to_string(), Some(10))
        .await?;

    if let ToolOutput::Success(r) = result {
        for content in &r.content {
            if let Content::Text(tc) = content {
                println!("Query result:\n{}", tc.text);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_tables() {
        let db = Arc::new(Database::new());
        let server = DatabaseServer::new(db);

        let result = server.list_tables().await;
        if let ToolOutput::Success(r) = result {
            assert!(!r.content.is_empty());
        }
    }

    #[tokio::test]
    async fn test_query_validation() {
        let db = Arc::new(Database::new());
        let server = DatabaseServer::new(db);

        // Valid SELECT should work
        let result = server.query("SELECT * FROM users".to_string(), None).await;
        assert!(result.is_ok());

        // INSERT should be rejected
        let result = server
            .query("INSERT INTO users VALUES (1)".to_string(), None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_describe_nonexistent_table() {
        let db = Arc::new(Database::new());
        let server = DatabaseServer::new(db);

        let result = server.describe_table("nonexistent".to_string()).await;
        assert!(result.is_err());
    }
}
