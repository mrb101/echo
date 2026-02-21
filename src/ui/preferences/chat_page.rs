use adw::prelude::*;
use relm4::prelude::*;

use crate::services::settings::AppSettings;

pub struct ChatPage {
    settings: AppSettings,
    temp_scale: gtk::Scale,
    system_prompt_buffer: gtk::TextBuffer,
    iterations_spin: gtk::SpinButton,
}

#[derive(Debug)]
pub enum ChatPageMsg {
    SetStreamingToggled(bool),
    SetSendWithEnter(bool),
    TemperatureChanged,
    SystemPromptChanged,
    SetAgenticEnabled(bool),
    MaxIterationsChanged,
    SetAutoApproveReadTools(bool),
}

#[derive(Debug)]
pub enum ChatPageOutput {
    SettingsChanged(AppSettings),
}

#[relm4::component(pub)]
impl Component for ChatPage {
    type Init = AppSettings;
    type Input = ChatPageMsg;
    type Output = ChatPageOutput;
    type CommandOutput = ();

    view! {
        adw::PreferencesPage {
            set_title: "Chat",
            set_icon_name: Some("chat-symbolic"),

            adw::PreferencesGroup {
                set_title: "Behavior",

                #[name = "streaming_row"]
                adw::SwitchRow {
                    set_title: "Stream responses",
                    set_subtitle: "Show tokens as they arrive",
                    set_active: model.settings.stream_responses,
                    connect_active_notify[sender] => move |row| {
                        sender.input(ChatPageMsg::SetStreamingToggled(row.is_active()));
                    },
                },

                #[name = "send_key_row"]
                adw::SwitchRow {
                    set_title: "Send with Enter",
                    set_subtitle: "When off, use Ctrl+Enter to send",
                    set_active: model.settings.send_with_enter,
                    connect_active_notify[sender] => move |row| {
                        sender.input(ChatPageMsg::SetSendWithEnter(row.is_active()));
                    },
                },
            },

            adw::PreferencesGroup {
                set_title: "Defaults",

                #[local_ref]
                temp_row -> adw::ActionRow {
                    set_title: "Temperature",
                    set_subtitle: "Controls response randomness (0.0 = focused, 2.0 = creative)",
                },
            },

            #[local_ref]
            system_prompt_group -> adw::PreferencesGroup {
                set_title: "System Prompt",
                set_description: Some("Default instructions sent to the AI for all conversations"),
            },

            adw::PreferencesGroup {
                set_title: "Agent",
                set_description: Some("Settings for the AI agent that can use tools"),

                #[name = "agentic_row"]
                adw::SwitchRow {
                    set_title: "Enable agentic mode",
                    set_subtitle: "Allow AI to use tools (file read/write, shell, web fetch)",
                    set_active: model.settings.agentic_enabled,
                    connect_active_notify[sender] => move |row| {
                        sender.input(ChatPageMsg::SetAgenticEnabled(row.is_active()));
                    },
                },

                #[local_ref]
                iterations_row -> adw::ActionRow {
                    set_title: "Max agent iterations",
                    set_subtitle: "Maximum number of tool-use loops per request",
                },

                #[name = "auto_approve_row"]
                adw::SwitchRow {
                    set_title: "Auto-approve read tools",
                    set_subtitle: "Skip approval for safe, read-only tools",
                    set_active: model.settings.auto_approve_read_tools,
                    connect_active_notify[sender] => move |row| {
                        sender.input(ChatPageMsg::SetAutoApproveReadTools(row.is_active()));
                    },
                },
            },
        }
    }

    fn init(
        settings: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let temp_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 2.0, 0.1);
        temp_scale.set_value(settings.temperature as f64);
        temp_scale.set_width_request(200);
        temp_scale.set_valign(gtk::Align::Center);
        temp_scale.set_draw_value(true);
        temp_scale.set_digits(1);
        temp_scale.set_value_pos(gtk::PositionType::Left);

        let sender_temp = sender.input_sender().clone();
        temp_scale.connect_value_changed(move |_| {
            sender_temp.send(ChatPageMsg::TemperatureChanged).unwrap();
        });

        let temp_row = adw::ActionRow::new();

        // System prompt text view
        let system_prompt_buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        if let Some(prompt) = &settings.default_system_prompt {
            system_prompt_buffer.set_text(prompt);
        }

        let system_prompt_view = gtk::TextView::builder()
            .buffer(&system_prompt_buffer)
            .wrap_mode(gtk::WrapMode::WordChar)
            .top_margin(8)
            .bottom_margin(8)
            .left_margin(8)
            .right_margin(8)
            .height_request(100)
            .build();
        system_prompt_view.add_css_class("card");

        let system_prompt_group = adw::PreferencesGroup::new();

        let sender_sp = sender.input_sender().clone();
        system_prompt_buffer.connect_changed(move |_| {
            sender_sp.send(ChatPageMsg::SystemPromptChanged).unwrap();
        });

        // Agent iterations spin button
        let iterations_spin = gtk::SpinButton::with_range(1.0, 50.0, 1.0);
        iterations_spin.set_value(settings.max_agent_iterations as f64);
        iterations_spin.set_valign(gtk::Align::Center);

        let sender_iter = sender.input_sender().clone();
        iterations_spin.connect_value_changed(move |_| {
            sender_iter.send(ChatPageMsg::MaxIterationsChanged).unwrap();
        });

        let iterations_row = adw::ActionRow::new();

        let model = Self {
            settings,
            temp_scale: temp_scale.clone(),
            system_prompt_buffer: system_prompt_buffer.clone(),
            iterations_spin: iterations_spin.clone(),
        };

        let widgets = view_output!();

        // Add scale as suffix imperatively
        widgets.temp_row.add_suffix(&temp_scale);

        // Add text view to system prompt group
        widgets.system_prompt_group.add(&system_prompt_view);

        // Add spin button to iterations row
        widgets.iterations_row.add_suffix(&iterations_spin);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            ChatPageMsg::SetStreamingToggled(active) => {
                self.settings.stream_responses = active;
                let _ = sender.output(ChatPageOutput::SettingsChanged(self.settings.clone()));
            }
            ChatPageMsg::SetSendWithEnter(active) => {
                self.settings.send_with_enter = active;
                let _ = sender.output(ChatPageOutput::SettingsChanged(self.settings.clone()));
            }
            ChatPageMsg::TemperatureChanged => {
                self.settings.temperature = self.temp_scale.value() as f32;
                let _ = sender.output(ChatPageOutput::SettingsChanged(self.settings.clone()));
            }
            ChatPageMsg::SystemPromptChanged => {
                let start = self.system_prompt_buffer.start_iter();
                let end = self.system_prompt_buffer.end_iter();
                let text = self
                    .system_prompt_buffer
                    .text(&start, &end, false)
                    .to_string();
                self.settings.default_system_prompt = if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                };
                let _ = sender.output(ChatPageOutput::SettingsChanged(self.settings.clone()));
            }
            ChatPageMsg::SetAgenticEnabled(active) => {
                self.settings.agentic_enabled = active;
                let _ = sender.output(ChatPageOutput::SettingsChanged(self.settings.clone()));
            }
            ChatPageMsg::MaxIterationsChanged => {
                self.settings.max_agent_iterations = self.iterations_spin.value() as u32;
                let _ = sender.output(ChatPageOutput::SettingsChanged(self.settings.clone()));
            }
            ChatPageMsg::SetAutoApproveReadTools(active) => {
                self.settings.auto_approve_read_tools = active;
                let _ = sender.output(ChatPageOutput::SettingsChanged(self.settings.clone()));
            }
        }
    }
}
