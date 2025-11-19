use serde::{Deserialize, Serialize};
use serde_json::Value;

// Re-export official ACP types
pub use agent_client_protocol::SessionNotification;

/// JSON-RPC message structure
///
/// This represents both requests and notifications in the Agent Communication Protocol.
/// All messages flow between the IDE client and the AI agent through this format.
///
/// Note: We maintain this wrapper for stdio-based JSON-RPC handling, as the official
/// agent-client-protocol crate focuses on the protocol types rather than transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// Client information from the initialize request
///
/// This tells us which IDE is connecting (Zed, Neovim, VSCode, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Name of the client (e.g., "zed", "neovim")
    pub name: String,
    /// Display title of the client (e.g., "Zed")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Version of the client
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Agent information from the initialize response
///
/// This tells us which AI agent is responding (Claude Code, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Name of the agent (e.g., "@zed-industries/claude-code-acp")
    pub name: String,
    /// Display title of the agent (e.g., "Claude Code")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Version of the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Initialize request parameters
///
/// Sent by the IDE during the initialization handshake.
/// We extract the clientInfo to determine which IDE is connecting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    /// Information about the client (IDE)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_info: Option<ClientInfo>,
    /// Protocol version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// Additional capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_rpc_request() {
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: Some("initialize".to_string()),
            params: Some(json!({
                "clientInfo": {
                    "name": "zed",
                    "version": "0.1.0"
                }
            })),
            result: None,
            error: None,
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: JsonRpcMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.jsonrpc, "2.0");
        assert_eq!(deserialized.method.unwrap(), "initialize");
    }

    #[test]
    fn test_json_rpc_notification() {
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some("session/update".to_string()),
            params: Some(json!({
                "sessionId": "test-session",
                "update": {}
            })),
            result: None,
            error: None,
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        assert!(!serialized.contains("\"id\""));
    }

    #[test]
    fn test_initialize_request_parsing() {
        let params = json!({
            "clientInfo": {
                "name": "zed",
                "version": "0.1.0"
            },
            "protocolVersion": "1.0",
            "capabilities": {}
        });

        let init_req: InitializeRequest = serde_json::from_value(params).unwrap();
        assert!(init_req.client_info.is_some());

        let client_info = init_req.client_info.unwrap();
        assert_eq!(client_info.name, "zed");
        assert_eq!(client_info.version.unwrap(), "0.1.0");
    }

    #[test]
    fn test_initialize_request_minimal() {
        let params = json!({});
        let init_req: InitializeRequest = serde_json::from_value(params).unwrap();
        assert!(init_req.client_info.is_none());
    }

    #[test]
    fn test_client_info_deserialization() {
        let json = json!({
            "name": "neovim",
            "version": "0.9.0"
        });

        let client_info: ClientInfo = serde_json::from_value(json).unwrap();
        assert_eq!(client_info.name, "neovim");
        assert_eq!(client_info.version.unwrap(), "0.9.0");
    }
}
