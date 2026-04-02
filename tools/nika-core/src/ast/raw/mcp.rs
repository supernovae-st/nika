//! Raw MCP configuration AST.

use indexmap::IndexMap;

use crate::source::Spanned;

/// Raw MCP configuration block.
#[derive(Debug, Clone, Default)]
pub struct RawMcpConfig {
    /// Server definitions: name -> config
    pub servers: IndexMap<Spanned<String>, Spanned<RawMcpServer>>,
}

/// Raw MCP server configuration.
#[derive(Debug, Clone, Default)]
pub struct RawMcpServer {
    /// Command to spawn the server
    pub command: Option<Spanned<String>>,

    /// Reference source: "config", "project", "global" (resolve from config files)
    pub from: Option<Spanned<String>>,

    /// Arguments for the command
    pub args: Option<Spanned<Vec<Spanned<String>>>>,

    /// Environment variables
    pub env: Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>,

    /// Working directory
    pub cwd: Option<Spanned<String>>,

    /// URL for SSE transport
    pub url: Option<Spanned<String>>,

    /// Transport type: stdio (default) or sse
    pub transport: Option<Spanned<String>>,
}

impl RawMcpConfig {
    /// Create a new empty MCP configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the number of configured servers.
    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    /// Get a server by name.
    pub fn get_server(&self, name: &str) -> Option<&Spanned<RawMcpServer>> {
        self.servers
            .iter()
            .find(|(k, _)| k.value == name)
            .map(|(_, v)| v)
    }

    /// Check if a server is configured.
    pub fn has_server(&self, name: &str) -> bool {
        self.get_server(name).is_some()
    }

    /// Iterate over server names.
    pub fn server_names(&self) -> impl Iterator<Item = &str> {
        self.servers.keys().map(|k| k.value.as_str())
    }
}

impl RawMcpServer {
    /// Create a new server config with a command.
    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: Some(Spanned::dummy(command.into())),
            ..Default::default()
        }
    }

    /// Create a new server config with a URL (SSE transport).
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            url: Some(Spanned::dummy(url.into())),
            transport: Some(Spanned::dummy("sse".to_string())),
            ..Default::default()
        }
    }

    /// Create a new server config with a from: reference.
    pub fn with_from(source: impl Into<String>) -> Self {
        Self {
            from: Some(Spanned::dummy(source.into())),
            ..Default::default()
        }
    }

    /// Check if this is a reference (has from: field).
    pub fn is_reference(&self) -> bool {
        self.from.is_some()
    }

    /// Check if this is an SSE server.
    pub fn is_sse(&self) -> bool {
        self.url.is_some()
            || self
                .transport
                .as_ref()
                .map(|t| t.value == "sse")
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{FileId, Span};

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    #[test]
    fn test_mcp_config_empty() {
        let config = RawMcpConfig::new();
        assert_eq!(config.server_count(), 0);
        assert!(!config.has_server("novanet"));
    }

    #[test]
    fn test_mcp_config_with_servers() {
        let mut config = RawMcpConfig::new();

        config.servers.insert(
            Spanned::new("novanet".to_string(), make_span(0, 7)),
            Spanned::new(
                RawMcpServer::with_command("cargo run -p novanet-mcp"),
                make_span(10, 50),
            ),
        );

        config.servers.insert(
            Spanned::new("external".to_string(), make_span(60, 68)),
            Spanned::new(
                RawMcpServer::with_url("http://localhost:8080"),
                make_span(70, 100),
            ),
        );

        assert_eq!(config.server_count(), 2);
        assert!(config.has_server("novanet"));
        assert!(config.has_server("external"));
        assert!(!config.has_server("unknown"));

        let names: Vec<&str> = config.server_names().collect();
        assert!(names.contains(&"novanet"));
        assert!(names.contains(&"external"));
    }

    #[test]
    fn test_mcp_server_sse() {
        let stdio_server = RawMcpServer::with_command("cargo run");
        assert!(!stdio_server.is_sse());

        let sse_server = RawMcpServer::with_url("http://localhost:8080");
        assert!(sse_server.is_sse());
    }

    #[test]
    fn test_mcp_server_from_reference() {
        let server = RawMcpServer::with_from("config");
        assert!(server.is_reference());
        assert!(server.command.is_none());
        assert_eq!(server.from.as_ref().unwrap().value, "config");
    }

    #[test]
    fn test_mcp_server_inline_is_not_reference() {
        let server = RawMcpServer::with_command("npx");
        assert!(!server.is_reference());
        assert!(server.from.is_none());
    }

    #[test]
    fn test_mcp_server_from_with_overrides() {
        let mut server = RawMcpServer::with_from("config");
        server.env = Some(Spanned::new(
            {
                let mut env = IndexMap::new();
                env.insert(
                    Spanned::new("KEY".to_string(), make_span(0, 3)),
                    Spanned::new("val".to_string(), make_span(5, 8)),
                );
                env
            },
            make_span(0, 10),
        ));
        assert!(server.is_reference());
        assert!(server.env.is_some());
    }
}
