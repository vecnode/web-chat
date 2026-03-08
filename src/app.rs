use eframe::egui;
use egui::{Align, Frame, Layout};

use crate::chat::ChatExample;
use crate::ollama::{OllamaController, OllamaStatus};
use crate::mcp::{MCPController, MCPStatus};

use std::sync::{Arc, Mutex};

pub struct MyApp {
    pub chat: ChatExample,
    pub selected_model: String,
    server_status: ServerStatus,
    pub server_enabled: Arc<Mutex<bool>>,
    ollama: OllamaController,
    pub selected_ollama_model: Arc<Mutex<String>>,
    ollama_input_text: String,
    ollama_token_limit: i32,
    ollama_token_limit_enabled: bool,
    chat_token_limit: i32,
    chat_token_limit_enabled: bool,
    pub mcp: MCPController,
    inspector_visible: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum ServerStatus {
    Running,
    Stopped,
}

impl Default for MyApp {
    fn default() -> Self {
        let ollama = OllamaController::new();
        ollama.check_status(); // Check Ollama status on startup
        ollama.fetch_models(); // Fetch available models on startup
        let mcp = MCPController::new();
        mcp.set_ollama(ollama.clone());
        // MCP starts enabled by default, but we need to wait for chat sender to be set
        // before actually starting the server (which happens in main.rs)
        
        // Note: chat_sender will be set in main.rs after chat is created
        
        Self {
            chat: ChatExample::default(),
            selected_model: String::new(),
            server_status: ServerStatus::Running, // Assume running since server starts before UI
            server_enabled: Arc::new(Mutex::new(true)),
            ollama,
            selected_ollama_model: Arc::new(Mutex::new(String::new())),
            ollama_input_text: String::new(),
            ollama_token_limit: 70,
            ollama_token_limit_enabled: false,
            chat_token_limit: 70,
            chat_token_limit_enabled: false,
            mcp,
            inspector_visible: false,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_width = ui.available_width();
            let left_width = 200.0; // Fixed width of 200px
            let right_width = 200.0; // Fixed width of 200px
            // Adjust center and right widths based on inspector visibility
            let (center_width, right_width) = if self.inspector_visible {
                // Ensure widths are never negative, subtract 20px for testing
                let center = (available_width - left_width - right_width - 10.0).max(0.0);
                (center, right_width)
            } else {
                // Ensure center width is never negative, subtract 20px for testing
                ((available_width - left_width - 10.0).max(0.0), 0.0) // Chat takes remaining space when inspector is hidden
            };
            let available_height = ui.available_height();
            
            let light_gray_bg = egui::Color32::from_rgb(40, 40, 40);
            
            ui.horizontal(|ui| {
                // Left column - 20% width
                Frame::default()
                    .fill(light_gray_bg)
                    .inner_margin(0.0)
                    .outer_margin(0.0)
                    .show(ui, |ui| {
                        ui.set_width(left_width);
                        ui.set_height(available_height);
                        ui.vertical(|ui| {
                            // Chat Status at the top
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new("Chat Status").strong());
                                    ui.add_space(4.0);
                                    
                                    // Chat 1 button - 40% of column width
                                    let button_width = left_width * 0.4;
                                    ui.add_sized([button_width, 0.0], egui::Button::new("Chat 1"));
                                    
                                    // Separator
                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.add_space(8.0);
                                    
                                    // Communication header with green border - 100% width
                                    let comm_available_width = ui.available_width() - 16.0; // Account for left/right margins
                                    egui::Frame::default()
                                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 255, 0)))
                                        .inner_margin(egui::Margin { left: 6.0, right: 6.0, top: 6.0, bottom: 6.0 })
                                        .rounding(4.0)
                                        .show(ui, |ui| {
                                            ui.set_width(comm_available_width);
                                            ui.vertical(|ui| {
                                                ui.label(egui::RichText::new("Communication").strong());
                                                ui.add_space(4.0);
                                                
                                                // Server Status
                                                ui.label(egui::RichText::new("Server Status").strong());
                                                ui.add_space(4.0);
                                                
                                                let (status_text, status_color) = match self.server_status {
                                                    ServerStatus::Running => ("● Running", egui::Color32::from_rgb(0, 255, 0)),
                                                    ServerStatus::Stopped => ("● Stopped", egui::Color32::from_rgb(255, 0, 0)),
                                                };
                                                
                                                ui.label(egui::RichText::new(status_text).color(status_color));
                                                ui.add_space(4.0);
                                                
                                                // ON/OFF button
                                                let is_enabled = *self.server_enabled.lock().unwrap();
                                                let button_text = if is_enabled { "OFF" } else { "ON" };
                                                
                                                if ui.button(button_text).clicked() {
                                                    let mut enabled = self.server_enabled.lock().unwrap();
                                                    *enabled = !*enabled;
                                                    self.server_status = if *enabled {
                                                        ServerStatus::Running
                                                    } else {
                                                        ServerStatus::Stopped
                                                    };
                                                }
                                                
                                                ui.add_space(2.0);
                                                ui.label(egui::RichText::new("http://127.0.0.1:3000").small().weak());
                                            });
                                        });
                                    
                                    // Separator
                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.add_space(8.0);
                                    
                                    // Ollama Status
                                    ui.label(egui::RichText::new("Ollama Status").strong());
                                    ui.add_space(4.0);
                                    
                                    let ollama_status = self.ollama.status();
                                    let (ollama_status_text, ollama_status_color) = match ollama_status {
                                        OllamaStatus::Running => ("● Running", egui::Color32::from_rgb(0, 255, 0)),
                                        OllamaStatus::Stopped => ("● Stopped", egui::Color32::from_rgb(255, 0, 0)),
                                        OllamaStatus::Checking => ("● Checking", egui::Color32::from_rgb(255, 255, 0)),
                                    };
                                    
                                    ui.label(egui::RichText::new(ollama_status_text).color(ollama_status_color));
                                    ui.add_space(4.0);
                                    
                                    // Check button
                                    if ui.button("Check").clicked() {
                                        self.ollama.check_status();
                                        self.ollama.fetch_models();
                                    }
                                    
                                    ui.add_space(2.0);
                                    ui.label(egui::RichText::new("http://127.0.0.1:11434").small().weak());
                                    ui.add_space(4.0);
                                    
                                    // Model combobox
                                    let ollama_status = self.ollama.status();
                                    let models = self.ollama.models();
                                    let current_model = self.selected_ollama_model.lock().unwrap().clone();
                                    
                                    if ollama_status == OllamaStatus::Running && !models.is_empty() {
                                        egui::ComboBox::from_id_source("ollama_model_selector")
                                            .selected_text(if current_model.is_empty() {
                                                "Select model"
                                            } else {
                                                &current_model
                                            })
                                            .show_ui(ui, |ui| {
                                                for model in &models {
                                                    if ui.selectable_label(current_model == *model, model).clicked() {
                                                        *self.selected_ollama_model.lock().unwrap() = model.clone();
                                                    }
                                                }
                                            });
                                    } else if ollama_status == OllamaStatus::Stopped {
                                        ui.label(egui::RichText::new("Not installed").small().weak());
                                    } else if ollama_status == OllamaStatus::Checking {
                                        ui.label(egui::RichText::new("Checking...").small().weak());
                                    } else {
                                        ui.label(egui::RichText::new("No models available").small().weak());
                                    }
                                    
                                    // Token limit toggle and input
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut self.ollama_token_limit_enabled, "Token Limit");
                                        if self.ollama_token_limit_enabled {
                                            ui.label("Tokens:");
                                            ui.add_sized(
                                                [40.0, 18.0],
                                                egui::DragValue::new(&mut self.ollama_token_limit)
                                                    .range(1..=1000)
                                                    .speed(1.0),
                                            );
                                        }
                                    });
                                    
                                    // Input field and Send button (always visible)
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        // Input field uses ~40% of the available width
                                        let total_width = ui.available_width();
                                        let input_width = total_width * 0.4;
                                        
                                        // Use consistent height for alignment (button height is typically ~20-24px)
                                        let widget_height = 20.0;
                                        let response = ui.add_sized(
                                            [input_width, widget_height],
                                            egui::TextEdit::singleline(&mut self.ollama_input_text),
                                        );

                                        let can_send = ollama_status == OllamaStatus::Running && !current_model.is_empty();

                                        let enter_pressed = response.lost_focus()
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter));

                                        let send_clicked = ui.button("Send").clicked();

                                        if (enter_pressed || send_clicked)
                                            && can_send
                                            && !self.ollama_input_text.trim().is_empty()
                                        {
                                            let model = current_model.clone();
                                            let message = self.ollama_input_text.clone();
                                            let token_limit = if self.ollama_token_limit_enabled {
                                                Some(self.ollama_token_limit)
                                            } else {
                                                None
                                            };
                                            let tx = self.chat.inbox().sender();
                                            self.ollama.send_message(
                                                model,
                                                message,
                                                token_limit,
                                                Box::new(move |msg| {
                                                    tx.send(msg).ok();
                                                }),
                                            );
                                            self.ollama_input_text.clear();
                                        }
                                    });

                                    ui.add_space(8.0);
                                    ui.separator();
                                    
                                    // MCP Status
                                    ui.add_space(8.0);
                                    ui.label(egui::RichText::new("MCP Status").strong());
                                    ui.add_space(4.0);
                                    
                                    let mcp_status = self.mcp.status();
                                    let (mcp_status_text, mcp_status_color) = match mcp_status {
                                        MCPStatus::Running => ("● Running", egui::Color32::from_rgb(0, 255, 0)),
                                        MCPStatus::Stopped => ("● Stopped", egui::Color32::from_rgb(255, 0, 0)),
                                        MCPStatus::Checking => ("● Checking", egui::Color32::from_rgb(255, 255, 0)),
                                    };
                                    
                                    ui.label(egui::RichText::new(mcp_status_text).color(mcp_status_color));
                                    ui.add_space(4.0);
                                    
                                    // ON/OFF button
                                    let is_enabled = *self.mcp.enabled().lock().unwrap();
                                    let button_text = if is_enabled { "OFF" } else { "ON" };
                                    
                                    if ui.button(button_text).clicked() {
                                        let new_state = !is_enabled;
                                        self.mcp.set_enabled(new_state);
                                    }
                                });
                            });
                        });
                    });
                
                // Center column - 60% width (top bar + chat area)
                ui.vertical(|ui| {
                    ui.set_min_width(center_width);
                    ui.set_max_width(center_width);
                    
                    // Top bar - 20% of window height
                    let top_bar_height = available_height * 0.08;
                    // Use center_width minus 10px to match chat area and adapt to inspector visibility
                    let top_bar_width = center_width;

                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.set_min_width(top_bar_width);
                        ui.set_max_width(top_bar_width);
                        Frame::default()
                            .fill(light_gray_bg)
                            .inner_margin(0.0)
                            .outer_margin(0.0)
                            .show(ui, |ui| {
                                ui.set_min_width(top_bar_width);
                                ui.set_max_width(top_bar_width);
                                ui.set_height(top_bar_height);
                                ui.horizontal(|ui| {
                                    ui.set_min_width(top_bar_width);
                                    ui.set_max_width(top_bar_width);
                                    // Left side - Models section, with left margin
                                    ui.horizontal(|ui| {
                                        // Left margin inside top bar column
                                        ui.add_space(8.0);
                                        ui.vertical(|ui| {
                                            ui.add_space(6.0);
                                            
                                            ui.label(egui::RichText::new("Chat Model").strong());
                                            ui.add_space(4.0);

                                            // Use Ollama models for the combobox
                                            let ollama_models = self.ollama.models();
                                            let ollama_status = self.ollama.status();
                                            
                                            if ollama_status == OllamaStatus::Running && !ollama_models.is_empty() {
                                                egui::ComboBox::from_id_source("model_selector")
                                                    .selected_text(if self.selected_model.is_empty() {
                                                        "Select model"
                                                    } else {
                                                        &self.selected_model
                                                    })
                                                    .show_ui(ui, |ui| {
                                                        for model in &ollama_models {
                                                            if ui.selectable_label(self.selected_model == *model, model).clicked() {
                                                                self.selected_model = model.clone();
                                                            }
                                                        }
                                                    });
                                            } else {
                                                ui.label(egui::RichText::new("No models available").small().weak());
                                            }
                                            
                                            // Token limit toggle and input for main chat
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.checkbox(&mut self.chat_token_limit_enabled, "Token Limit");
                                                if self.chat_token_limit_enabled {
                                                    ui.label("Tokens:");
                                                    ui.add_sized(
                                                        [40.0, 18.0],
                                                        egui::DragValue::new(&mut self.chat_token_limit)
                                                            .range(1..=1000)
                                                            .speed(1.0),
                                                    );
                                                }
                                            });
                                            ui.add_space(4.0);
                                        });
                                    });
                                    
                                    // Push Inspector button to the right edge
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.add_space(8.0);
                                        ui.vertical(|ui| {
                                            ui.add_space(6.0);
                                            
                                            if ui.button("Inspector").clicked() {
                                                self.inspector_visible = !self.inspector_visible;
                                            }
                                        });
                                    });
                                });
                            });
                    });
                    
                    ui.add_space(4.0);  
                    // Chat area - remaining height
                    Frame::default()
                        .fill(light_gray_bg)
                        .inner_margin(0.0)
                        .outer_margin(0.0)
                        .show(ui, |ui| {
                            ui.set_min_width(center_width);
                            ui.set_max_width(center_width);
                            ui.set_height(available_height - top_bar_height - 28.0); // 2px more height to match columns
                            
                            // Set up message handler for chat with current values
                            // Update each frame to ensure we have the latest model selection and settings
                            let selected_model = self.selected_model.clone();
                            let ollama_status = self.ollama.status();
                            let ollama_controller = self.ollama.clone();
                            let chat_token_limit = if self.chat_token_limit_enabled {
                                Some(self.chat_token_limit)
                            } else {
                                None
                            };
                            let tx = self.chat.inbox().sender();
                            
                            let waiting_flag = self.chat.waiting_for_response().clone();
                            self.chat.set_message_handler(Box::new(move |message: String| {
                                let tx_clone = tx.clone();
                                if selected_model.is_empty() {
                                    // No model selected, respond with "Please select a model"
                                    let bot_message = crate::chat::ChatMessage {
                                        content: "Please select a model".to_string(),
                                        from: Some("System".to_string()),
                                    };
                                    tx_clone.send(bot_message).ok();
                                } else if ollama_status == crate::ollama::OllamaStatus::Running {
                                    // Model selected and Ollama is running, send to Ollama
                                    // Set waiting flag to true
                                    *waiting_flag.lock().unwrap() = true;
                                    
                                    let model_clone = selected_model.clone();
                                    let tx_for_ollama = tx_clone.clone();
                                    let waiting_flag_clone = waiting_flag.clone();
                                    ollama_controller.send_message(
                                        model_clone,
                                        message,
                                        chat_token_limit,
                                        Box::new(move |msg| {
                                            // Clear waiting flag when response arrives
                                            *waiting_flag_clone.lock().unwrap() = false;
                                            tx_for_ollama.send(msg).ok();
                                        }),
                                    );
                                } else {
                                    // Ollama not running
                                    let bot_message = crate::chat::ChatMessage {
                                        content: "Ollama is not running. Please check Ollama status.".to_string(),
                                        from: Some("System".to_string()),
                                    };
                                    tx_clone.send(bot_message).ok();
                                }
                            }));
                            
                            self.chat.ui(ui);
                        });
                });
                
                // Right column - 20% width (only shown when inspector is visible)
                if self.inspector_visible {
                    Frame::default()
                        .fill(light_gray_bg)
                        .inner_margin(0.0)
                        .outer_margin(0.0)
                        .show(ui, |ui| {
                            ui.set_width(right_width);
                            ui.set_height(available_height);
                            // Right column content can be added here
                        });
                }
            });
        });
    }
}



