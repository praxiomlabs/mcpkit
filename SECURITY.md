# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security issue, please report it responsibly.

### How to Report

1. **Do NOT** open a public GitHub issue for security vulnerabilities
2. Email security concerns to: jkindrix@gmail.com
3. Or use GitHub's private vulnerability reporting feature

### What to Include

Please include the following in your report:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Any suggested fixes (optional)

### Response Timeline

- **Initial Response**: Within 48 hours
- **Status Update**: Within 7 days
- **Resolution Target**: Within 90 days (may vary based on severity)

### Disclosure Policy

- We follow coordinated disclosure practices
- We will credit reporters (unless they prefer to remain anonymous)
- We aim to release fixes before public disclosure

## Security Considerations for MCP

### Transport Security

- **Use TLS**: Always use `wss://` and `https://` in production
- **Validate Origins**: Verify WebSocket connection origins
- **Secure Tokens**: Never log or expose authentication tokens

### Tool Execution

- **Validate Inputs**: Always validate tool inputs before processing
- **Principle of Least Privilege**: Tools should request minimal permissions
- **Sandbox Dangerous Operations**: Isolate potentially harmful operations

### Resource Access

- **Access Control**: Implement proper authorization for resources
- **Path Traversal**: Validate and sanitize file paths
- **Sensitive Data**: Never expose credentials or secrets in resources

### Prompts

- **Injection Prevention**: Sanitize user inputs in prompt templates
- **Content Validation**: Validate prompt arguments

## Security Features

### Type Safety

The SDK uses Rust's type system to prevent common vulnerabilities:

- No unsafe code (`#![deny(unsafe_code)]`)
- Strong typing prevents type confusion
- Ownership system prevents use-after-free

### Error Handling

- Errors never expose sensitive information
- Stack traces are not sent to clients
- Structured error responses follow JSON-RPC spec

### Dependencies

- Minimal dependency tree
- Regular dependency audits via `cargo audit`
- CI includes security scanning

## Best Practices for Users

1. **Keep Updated**: Use the latest SDK version
2. **Audit Dependencies**: Run `cargo audit` regularly
3. **Secure Configuration**: Don't commit secrets to version control
4. **Monitor Logs**: Watch for suspicious activity
5. **Limit Permissions**: Use minimal required capabilities

## Security Checklist for Contributors

Before submitting PRs:

- [ ] No new `unsafe` code (or justified and reviewed)
- [ ] Input validation for all user-provided data
- [ ] No hardcoded credentials or secrets
- [ ] Error messages don't leak sensitive info
- [ ] Dependencies are from trusted sources
- [ ] No new security warnings from `cargo audit`

## Known Security Considerations

### MCP Protocol

The MCP protocol has known security considerations documented in the [MCP Security Best Practices](https://modelcontextprotocol.io/specification/2025-11-25/basic/security):

- Tool descriptions should be treated as untrusted
- Explicit user consent required for tool execution
- LLM sampling requests require user approval

### This Implementation

- All transports support TLS
- No unsafe code in the crate
- Regular security audits via GitHub Actions

## References

- [MCP Security Specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/security)
- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
