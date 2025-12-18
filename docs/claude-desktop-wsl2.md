# Claude Desktop WSL2 Integration

This guide explains how to configure Claude Desktop on Windows to use mcpkit MCP servers running in WSL2.

## Prerequisites

- Windows 10/11 with WSL2 installed
- Claude Desktop installed on Windows
- mcpkit built in WSL2 (`cargo build --release`)

## Configuration

Edit your Claude Desktop configuration file:

**Location:** `%APPDATA%\Claude\claude_desktop_config.json`

### Filesystem Server Example

```json
{
  "mcpServers": {
    "mcpkit-filesystem": {
      "command": "wsl.exe",
      "args": [
        "bash",
        "-lc",
        "mkdir -p /tmp/mcpkit-sandbox && /home/YOUR_USERNAME/path/to/mcpkit/target/release/filesystem-server /tmp/mcpkit-sandbox"
      ]
    }
  }
}
```

**Important:** Replace `/home/YOUR_USERNAME/path/to/mcpkit/` with your actual WSL2 path.

### Multiple Servers

You can configure multiple MCP servers:

```json
{
  "mcpServers": {
    "mcpkit-filesystem": {
      "command": "wsl.exe",
      "args": [
        "bash",
        "-lc",
        "mkdir -p /tmp/mcpkit-sandbox && /path/to/filesystem-server /tmp/mcpkit-sandbox"
      ]
    },
    "mcpkit-database": {
      "command": "wsl.exe",
      "args": [
        "bash",
        "-lc",
        "/path/to/database-server"
      ]
    }
  }
}
```

## Applying Changes

After editing the configuration:

1. **Fully quit** Claude Desktop (check the system tray - don't just close the window)
2. Restart Claude Desktop
3. The MCP server tools should now appear in your conversations

## Troubleshooting

### Server Disconnected Error

If you see "Server disconnected" warnings:

1. **Verify the path exists:**
   ```bash
   ls -la /path/to/target/release/filesystem-server
   ```

2. **Ensure sandbox directory exists:**
   ```bash
   mkdir -p /tmp/mcpkit-sandbox
   ```

3. **Test the command manually from Windows CMD:**
   ```cmd
   wsl.exe bash -lc "echo test && /path/to/filesystem-server /tmp/mcpkit-sandbox"
   ```

4. **Check Claude Desktop logs:**
   - Location: `%APPDATA%\Claude\logs\`

### Common Issues

| Issue | Solution |
|-------|----------|
| "Could not attach to MCP server" | Use `bash -lc` wrapper instead of `-e` |
| Path not found | Use full absolute WSL2 path |
| Permission denied | Run `chmod +x` on the binary |
| Server crashes immediately | Ensure sandbox directory exists |

## Verified Configurations

Both of the following configurations have been tested and verified working:

### Option 1: Login shell with directory creation (recommended)

```json
{
  "mcpServers": {
    "mcpkit-filesystem": {
      "command": "wsl.exe",
      "args": [
        "bash",
        "-lc",
        "mkdir -p /tmp/mcpkit-sandbox && /home/jkindrix/dev/projects/mcp-servers/mcpkit/target/release/filesystem-server /tmp/mcpkit-sandbox"
      ]
    }
  }
}
```

### Option 2: Direct execution with exec

```json
{
  "mcpServers": {
    "mcpkit-filesystem": {
      "command": "wsl.exe",
      "args": [
        "--",
        "bash",
        "-c",
        "exec /home/jkindrix/dev/projects/mcp-servers/mcpkit/target/release/filesystem-server /tmp/mcpkit-sandbox"
      ]
    }
  }
}
```

**Note:** Option 2 requires the sandbox directory to already exist.

## Available Tools

When connected, Claude Desktop will have access to:

| Tool | Description |
|------|-------------|
| `read_file` | Read file contents |
| `write_file` | Write content to a file |
| `append_file` | Append content to a file |
| `list_directory` | List directory contents |
| `get_metadata` | Get file/directory metadata |
| `search_files` | Search for files by pattern |
| `create_directory` | Create a new directory |
| `delete_file` | Delete a file |
| `delete_directory` | Delete an empty directory |
| `get_root` | Get the sandbox root directory path |

All operations are sandboxed to the specified root directory with path traversal protection.

## Additional Considerations

### Sandbox Persistence

The default sandbox location `/tmp/mcpkit-sandbox` is **volatile** - it's cleared when WSL restarts or the system reboots. For persistent storage, use a different location:

```json
{
  "mcpServers": {
    "mcpkit-filesystem": {
      "command": "wsl.exe",
      "args": [
        "bash", "-lc",
        "mkdir -p ~/mcpkit-sandbox && /path/to/filesystem-server ~/mcpkit-sandbox"
      ]
    }
  }
}
```

Or access Windows files directly:
```json
{
  "args": ["bash", "-lc", "mkdir -p /mnt/c/Users/YOU/mcpkit-sandbox && /path/to/filesystem-server /mnt/c/Users/YOU/mcpkit-sandbox"]
}
```

### Environment Variables

For servers that need environment variables (like database credentials):

```json
{
  "mcpServers": {
    "mcpkit-database": {
      "command": "wsl.exe",
      "args": [
        "bash", "-lc",
        "DATABASE_URL='sqlite:///tmp/test.db' /path/to/database-server"
      ]
    }
  }
}
```

### Multiple WSL Distributions

If you have multiple WSL distributions, specify which one to use:

```json
{
  "command": "wsl.exe",
  "args": ["-d", "Ubuntu", "bash", "-lc", "..."]
}
```

List available distributions with: `wsl -l -v`

### After Rebuilding the Server

When you rebuild the server (`cargo build --release`), Claude Desktop may cache the old connection. To pick up changes:

1. Fully quit Claude Desktop (check system tray)
2. Restart Claude Desktop
3. Start a new conversation

### Debugging Connection Issues

Claude Desktop logs are located at:
- **Windows:** `%APPDATA%\Claude\logs\`

Look for MCP-related entries to diagnose connection problems.
