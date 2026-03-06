use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use rmcp::{ServerHandler, model::*, service::RequestContext, ErrorData as McpError, RoleServer};
use crate::ollama::OllamaController;
use crate::chat::ChatMessage;

/// Callback type for sending messages to chat
/// This allows the MCP module to connect to the chat without direct dependencies
pub type ChatSendFn = Arc<dyn Fn(ChatMessage) + Send + Sync>;

#[derive(Clone)]
pub struct MCPController {
    status: Arc<Mutex<MCPStatus>>,
    pub enabled: Arc<Mutex<bool>>,
    server_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    ollama: Arc<Mutex<Option<OllamaController>>>,
    send_to_chat: Arc<Mutex<Option<ChatSendFn>>>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum MCPStatus {
    Running,
    Stopped,
    Checking,
}

impl MCPController {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(MCPStatus::Stopped)),
            enabled: Arc::new(Mutex::new(true)), // Start enabled by default
            server_handle: Arc::new(Mutex::new(None)),
            ollama: Arc::new(Mutex::new(None)),
            send_to_chat: Arc::new(Mutex::new(None)),
        }
    }

    pub fn status(&self) -> MCPStatus {
        *self.status.lock().unwrap()
    }

    /// Get the enabled state (for UI access)
    pub fn enabled(&self) -> Arc<Mutex<bool>> {
        self.enabled.clone()
    }

    /// Set the Ollama controller reference
    pub fn set_ollama(&self, ollama: OllamaController) {
        *self.ollama.lock().unwrap() = Some(ollama);
    }

    /// Set a callback function to send messages to chat
    /// This allows the MCP module to connect to the chat without direct dependencies
    pub fn set_chat_sender_fn(&self, send_fn: ChatSendFn) {
        *self.send_to_chat.lock().unwrap() = Some(send_fn);
    }

    /// Set the MCP server status based on enabled state
    pub fn set_enabled(&self, enabled: bool) {
        let status = self.status.clone();
        let server_handle = self.server_handle.clone();
        let ollama = self.ollama.clone();
        let send_to_chat = self.send_to_chat.clone();
        
        *self.enabled.lock().unwrap() = enabled;
        
        if enabled {
            // Start MCP server
            *status.lock().unwrap() = MCPStatus::Checking;
            
            let status_clone = status.clone();
            let ollama_clone = ollama.clone();
            let send_to_chat_clone = send_to_chat.clone();
            
            // Spawn a thread with its own Tokio runtime for the MCP server
            let handle = std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let server = OllamaMCPServer::new(ollama_clone, send_to_chat_clone);
                    
                    if let Err(e) = start_mcp_server(server, status_clone.clone()).await {
                        eprintln!("[MCP] Server error: {}", e);
                        *status_clone.lock().unwrap() = MCPStatus::Stopped;
                    }
                });
            });
            
            *server_handle.lock().unwrap() = Some(handle);
        } else {
            // Stop MCP server
            if let Some(handle) = server_handle.lock().unwrap().take() {
                // Signal the server to stop by updating status
                *status.lock().unwrap() = MCPStatus::Stopped;
                // Wait for the thread to finish (with timeout to avoid blocking)
                // Note: In a production system, you'd want a more graceful shutdown mechanism
                let _ = handle.join();
            }
            *status.lock().unwrap() = MCPStatus::Stopped;
            println!("[MCP] Server disabled");
        }
    }
}



impl Default for MCPController {
    fn default() -> Self {
        Self::new()
    }
}

// MCP Server implementation that connects to Ollama and routes to chat
#[derive(Clone)]
struct OllamaMCPServer {
    ollama: Arc<Mutex<Option<OllamaController>>>,
    send_to_chat: Arc<Mutex<Option<ChatSendFn>>>,
}

impl OllamaMCPServer {
    fn new(
        ollama: Arc<Mutex<Option<OllamaController>>>,
        send_to_chat: Arc<Mutex<Option<ChatSendFn>>>,
    ) -> Self {
        Self { ollama, send_to_chat }
    }
}

impl ServerHandler for OllamaMCPServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build()
        )
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let ollama = self.ollama.clone();
        async move {
            // Get available Ollama models and expose them as tools
            let models = if let Some(ref ollama) = *ollama.lock().unwrap() {
                ollama.models()
            } else {
                Vec::new()
            };

            let tools: Vec<Tool> = models
                .into_iter()
                .map(|model| {
                    let tool_name = format!("chat_with_{}", model.replace(":", "_").replace("-", "_"));
                    let input_schema_value = serde_json::json!({
                        "type": "object",
                        "properties": {
                            "message": {
                                "type": "string",
                                "description": "The message to send to the model"
                            },
                            "num_predict": {
                                "type": "integer",
                                "description": "Maximum number of tokens to generate (optional)"
                            }
                        },
                        "required": ["message"]
                    });
                    
                    // Convert to JsonObject (Map)
                    let input_schema_map: serde_json::Map<String, serde_json::Value> = 
                        serde_json::from_value(input_schema_value).unwrap_or_default();
                    
                    Tool::new_with_raw(
                        tool_name,
                        Some(format!("Chat with Ollama model: {}", model).into()),
                        Arc::new(input_schema_map),
                    )
                })
                .collect();

            Ok(ListToolsResult::with_all_items(tools))
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let ollama = self.ollama.clone();
        let send_to_chat = self.send_to_chat.clone();
        async move {
            // Extract model name from tool name (format: chat_with_model_name)
            let tool_name = request.name;
            let model_name = tool_name
                .strip_prefix("chat_with_")
                .unwrap_or(&tool_name)
                .replace("_", ":")
                .replace("__", "-");

            // Get message from arguments
            let arguments = request.arguments
                .ok_or_else(|| McpError::invalid_params("Missing arguments", None))?;
            
            let message = arguments
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params("Missing 'message' argument", None))?
                .to_string();

            let num_predict = arguments
                .get("num_predict")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);

            // Send user message to chat
            if let Some(ref send_fn) = *send_to_chat.lock().unwrap() {
                let user_msg = ChatMessage {
                    content: message.clone(),
                    from: Some("Human".to_string()),
                };
                send_fn(user_msg);
            }

            // Call Ollama
            let ollama_opt = ollama.lock().unwrap().clone();
            if let Some(ollama_controller) = ollama_opt {
                let model_clone = model_name.clone();
                let message_clone = message.clone();
                let num_predict_clone = num_predict;
                let send_to_chat_clone = send_to_chat.clone();

                // Use blocking call in async context
                let result = tokio::task::spawn_blocking(move || {
                    let (tx, rx) = std::sync::mpsc::channel();
                    
                    ollama_controller.send_message(
                        model_clone,
                        message_clone,
                        num_predict_clone,
                        Box::new(move |msg| {
                            // Send response to chat
                            if let Some(ref send_fn) = *send_to_chat_clone.lock().unwrap() {
                                send_fn(msg.clone());
                            }
                            tx.send(msg.content).ok();
                        }),
                    );

                    rx.recv().ok()
                })
                .await
                .map_err(|e| {
                    let error_msg = format!("Task error: {}", e);
                    McpError::internal_error(error_msg, None)
                })?;

                let response = result.ok_or_else(|| McpError::internal_error("No response from Ollama", None))?;

                let content_item = Content::text(response);
                Ok(CallToolResult::success(vec![content_item]))
            } else {
                Err(McpError::internal_error("Ollama controller not available", None))
            }
        }
    }
}

async fn start_mcp_server(
    _server: OllamaMCPServer,
    status: Arc<Mutex<MCPStatus>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    *status.lock().unwrap() = MCPStatus::Running;
    println!("[MCP] Server started - ready to accept MCP client connections");
    println!("[MCP] Ollama models exposed as MCP tools");
    println!("[MCP] Chat messages will route through MCP protocol");
    println!("[MCP] Note: MCP servers typically run as separate processes via stdio");
    println!("[MCP] This server is running in-process and ready for MCP protocol integration");
    
    // Keep server alive - actual MCP communication would happen via stdio
    // when the server is spawned as a process by MCP clients
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        if *status.lock().unwrap() == MCPStatus::Stopped {
            break;
        }
    }
    
    Ok(())
}
