use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::chat::ChatMessage;

const OLLAMA_URL: &str = "http://127.0.0.1:11434";

#[derive(Clone)]
pub struct OllamaController {
    status: Arc<Mutex<OllamaStatus>>,
    models: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum OllamaStatus {
    Running,
    Stopped,
    Checking,
}

impl OllamaController {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(OllamaStatus::Stopped)),
            models: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn status(&self) -> OllamaStatus {
        *self.status.lock().unwrap()
    }

    pub fn models(&self) -> Vec<String> {
        self.models.lock().unwrap().clone()
    }

    /// Trigger an async status check for Ollama and refresh models if running
    pub fn check_status(&self) {
        let status = self.status.clone();
        let models = self.models.clone();
        std::thread::spawn(move || {
            *status.lock().unwrap() = OllamaStatus::Checking;

            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(2))
                .build();

            let is_running = if let Ok(client) = client {
                client
                    .get(format!("{}/api/tags", OLLAMA_URL))
                    .send()
                    .is_ok()
            } else {
                false
            };

            let new_status = if is_running {
                OllamaStatus::Running
            } else {
                OllamaStatus::Stopped
            };
            *status.lock().unwrap() = new_status;

            if new_status == OllamaStatus::Running {
                fetch_models_inner(models);
            }
        });
    }

    /// Fetch available Ollama models
    pub fn fetch_models(&self) {
        let models = self.models.clone();
        std::thread::spawn(move || {
            fetch_models_inner(models);
        });
    }

    /// Send a message to Ollama using the selected model.
    /// `send_fn` is used to push messages into the UI inbox.
    pub fn send_message(
        &self,
        model: String,
        message: String,
        num_predict: Option<i32>,
        send_fn: Box<dyn Fn(ChatMessage) + Send + Sync>,
    ) {
        let model_clone = model.clone();
        std::thread::spawn(move || {
            println!("[Ollama] Sending message to model: {}", model_clone);
            if let Some(limit) = num_predict {
                println!("[Ollama] Token limit (num_predict): {}", limit);
            } else {
                println!("[Ollama] Token limit: disabled");
            }
            println!("[Ollama] Message: {}", message);

            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(60))
                .build();

            if let Ok(client) = client {
                // Note: User message is already added in chat.rs, so we don't add it here
                // Build request body conditionally based on whether num_predict is enabled
                let mut request_body = serde_json::json!({
                    "model": model_clone,
                    "messages": [
                        {
                            "role": "user",
                            "content": message
                        }
                    ],
                    "stream": false
                });
                
                // Only add num_predict to options if it's specified
                if let Some(limit) = num_predict {
                    request_body["options"] = serde_json::json!({
                        "num_predict": limit
                    });
                }

                println!("[Ollama] Request URL: {}/api/chat", OLLAMA_URL);
                println!("[Ollama] Request body: {}", serde_json::to_string_pretty(&request_body).unwrap_or_default());

                let response_result = client
                    .post(format!("{}/api/chat", OLLAMA_URL))
                    .json(&request_body)
                    .send();

                match response_result {
                    Ok(response) => {
                        let status = response.status();
                        println!("[Ollama] Response status: {} {}", status.as_u16(), status.as_str());
                        
                        if !status.is_success() {
                            println!("[Ollama] ERROR: HTTP request failed with status {}", status);
                            let error_text = response.text().ok();
                            if let Some(text) = error_text {
                                println!("[Ollama] Response body: {}", text);
                            }
                            // Send error message to clear the spinner
                            let error_msg = ChatMessage {
                                content: format!("Error: HTTP request failed with status {}", status),
                                from: Some("System".to_string()),
                            };
                            send_fn(error_msg);
                            return;
                        }

                        // Read response text first
                        let response_text = match response.text() {
                            Ok(text) => text,
                            Err(e) => {
                                println!("[Ollama] ERROR: Failed to read response text: {}", e);
                                // Send error message to clear the spinner
                                let error_msg = ChatMessage {
                                    content: format!("Error: Failed to read response: {}", e),
                                    from: Some("System".to_string()),
                                };
                                send_fn(error_msg);
                                return;
                            }
                        };

                        println!("[Ollama] Response text length: {} chars", response_text.len());

                        // Parse JSON
                        match serde_json::from_str::<serde_json::Value>(&response_text) {
                            Ok(json) => {
                                println!("[Ollama] Response JSON parsed successfully");
                                
                                // Check for error in response
                                if let Some(error) = json.get("error") {
                                    println!("[Ollama] ERROR in response: {}", error);
                                    let error_msg = if let Some(error_str) = error.as_str() {
                                        format!("Error: {}", error_str)
                                    } else {
                                        format!("Error: {}", error)
                                    };
                                    println!("[Ollama] Error message: {}", error_msg);
                                    // Send error message to clear the spinner
                                    let chat_error = ChatMessage {
                                        content: error_msg,
                                        from: Some("System".to_string()),
                                    };
                                    send_fn(chat_error);
                                    return;
                                }

                                if let Some(message_obj) = json.get("message") {
                                    println!("[Ollama] Message object found in response");
                                    println!("[Ollama] Message object structure: {}", serde_json::to_string_pretty(message_obj).unwrap_or_default());
                                    
                                    // Check if content exists and is not empty
                                    if let Some(content_value) = message_obj.get("content") {
                                        println!("[Ollama] Content field exists, type: {:?}", content_value);
                                        
                                        if let Some(content) = content_value.as_str() {
                                            println!("[Ollama] Content as string: '{}' ({} chars)", content, content.len());
                                            
                                            if content.is_empty() {
                                                println!("[Ollama] WARNING: Content is empty string!");
                                                // Try to get content from other possible fields (thinking, response, etc.)
                                                let alternative_content = message_obj.get("thinking")
                                                    .and_then(|t| t.as_str())
                                                    .or_else(|| json.get("response").and_then(|r| r.as_str()));
                                                
                                                if let Some(response_text) = alternative_content {
                                                    let field_name = if message_obj.get("thinking").is_some() {
                                                        "thinking"
                                                    } else {
                                                        "response"
                                                    };
                                                    println!("[Ollama] Found '{}' field instead: {} chars", field_name, response_text.len());
                                                    let from_text = format!("Ollama {}", model_clone);
                                                    let ollama_msg = ChatMessage {
                                                        content: response_text.to_string(),
                                                        from: Some(from_text),
                                                    };
                                                    send_fn(ollama_msg);
                                                    println!("[Ollama] Message sent to chat UI (from '{}' field)", field_name);
                                                } else {
                                                    println!("[Ollama] ERROR: Content is empty and no alternative field found");
                                                    // Send error message to clear the spinner
                                                    let error_msg = ChatMessage {
                                                        content: "Error: Empty response from Ollama".to_string(),
                                                        from: Some("System".to_string()),
                                                    };
                                                    send_fn(error_msg);
                                                }
                                            } else {
                                                println!("[Ollama] SUCCESS: Message content extracted ({} chars)", content.len());
                                                // Include model name in the from field: "Ollama model-name"
                                                let from_text = format!("Ollama {}", model_clone);
                                                let ollama_msg = ChatMessage {
                                                    content: content.to_string(),
                                                    from: Some(from_text),
                                                };
                                                send_fn(ollama_msg);
                                                println!("[Ollama] Message sent to chat UI");
                                            }
                                        } else {
                                            println!("[Ollama] ERROR: Content field is not a string");
                                            println!("[Ollama] Content value: {:?}", content_value);
                                            // Send error message to clear the spinner
                                            let error_msg = ChatMessage {
                                                content: "Error: Invalid content format from Ollama".to_string(),
                                                from: Some("System".to_string()),
                                            };
                                            send_fn(error_msg);
                                        }
                                    } else {
                                        println!("[Ollama] ERROR: No 'content' field in message object");
                                        println!("[Ollama] Message object: {}", serde_json::to_string_pretty(message_obj).unwrap_or_default());
                                        // Send error message to clear the spinner
                                        let error_msg = ChatMessage {
                                            content: "Error: Invalid response format from Ollama".to_string(),
                                            from: Some("System".to_string()),
                                        };
                                        send_fn(error_msg);
                                    }
                                } else {
                                    println!("[Ollama] ERROR: No 'message' field in response");
                                    println!("[Ollama] Full response: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
                                    // Send error message to clear the spinner
                                    let error_msg = ChatMessage {
                                        content: "Error: Invalid response format from Ollama".to_string(),
                                        from: Some("System".to_string()),
                                    };
                                    send_fn(error_msg);
                                }
                            }
                            Err(e) => {
                                println!("[Ollama] ERROR: Failed to parse JSON response: {}", e);
                                println!("[Ollama] Response text (first 500 chars): {}", 
                                    response_text.chars().take(500).collect::<String>());
                            }
                        }
                    }
                    Err(e) => {
                        println!("[Ollama] ERROR: HTTP request failed: {}", e);
                        println!("[Ollama] Error details: {:?}", e);
                        // Send error message to clear the spinner
                        let error_msg = ChatMessage {
                            content: format!("Error: HTTP request failed: {}", e),
                                from: Some("System".to_string()),
                        };
                        send_fn(error_msg);
                    }
                }
            } else {
                println!("[Ollama] ERROR: Failed to create HTTP client");
            }
        });
    }
}

impl Default for OllamaController {
    fn default() -> Self {
        Self::new()
    }
}

fn fetch_models_inner(models: Arc<Mutex<Vec<String>>>) {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build();

    let model_list = if let Ok(client) = client {
        if let Ok(response) = client
            .get(format!("{}/api/tags", OLLAMA_URL))
            .send()
        {
            if let Ok(json) = response.json::<serde_json::Value>() {
                if let Some(models_array) = json.get("models").and_then(|m| m.as_array()) {
                    models_array
                        .iter()
                        .filter_map(|m| {
                            m.get("name")
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    *models.lock().unwrap() = model_list;
}

