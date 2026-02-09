use adw::prelude::*;
use relm4::prelude::*;

pub struct SystemPromptDialog {
    buffer: gtk::TextBuffer,
    conversation_id: String,
}

#[derive(Debug)]
pub enum SystemPromptMsg {
    Save,
    Cancel,
    Clear,
}

#[derive(Debug)]
pub enum SystemPromptOutput {
    Updated(String, Option<String>), // (conversation_id, new_prompt)
    Cancelled,
}

pub struct SystemPromptInit {
    pub conversation_id: String,
    pub current_prompt: Option<String>,
}

#[relm4::component(pub, async)]
impl AsyncComponent for SystemPromptDialog {
    type Init = SystemPromptInit;
    type Input = SystemPromptMsg;
    type Output = SystemPromptOutput;
    type CommandOutput = ();

    view! {
        adw::Window {
            set_title: Some("System Prompt"),
            set_default_width: 500,
            set_default_height: 400,
            set_modal: true,

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    pack_start = &gtk::Button {
                        set_label: "Cancel",
                        connect_clicked => SystemPromptMsg::Cancel,
                    },
                    pack_end = &gtk::Button {
                        set_label: "Save",
                        add_css_class: "suggested-action",
                        connect_clicked => SystemPromptMsg::Save,
                    },
                },

                #[wrap(Some)]
                set_content = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_all: 12,

                    gtk::Label {
                        set_label: "Custom instructions for this conversation. Overrides the global default.",
                        set_wrap: true,
                        set_halign: gtk::Align::Start,
                        add_css_class: "dim-label",
                    },

                    gtk::ScrolledWindow {
                        set_vexpand: true,
                        set_hscrollbar_policy: gtk::PolicyType::Never,

                        #[name = "text_view"]
                        gtk::TextView {
                            set_wrap_mode: gtk::WrapMode::WordChar,
                            set_top_margin: 8,
                            set_bottom_margin: 8,
                            set_left_margin: 8,
                            set_right_margin: 8,
                            add_css_class: "card",
                        },
                    },

                    gtk::Button {
                        set_label: "Clear",
                        set_halign: gtk::Align::Start,
                        add_css_class: "destructive-action",
                        connect_clicked => SystemPromptMsg::Clear,
                    },
                },
            },
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        if let Some(prompt) = &init.current_prompt {
            buffer.set_text(prompt);
        }

        let model = Self {
            buffer: buffer.clone(),
            conversation_id: init.conversation_id,
        };

        let widgets = view_output!();
        widgets.text_view.set_buffer(Some(&buffer));

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            SystemPromptMsg::Save => {
                let start = self.buffer.start_iter();
                let end = self.buffer.end_iter();
                let text = self.buffer.text(&start, &end, false).to_string();
                let prompt = if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                };
                let _ = sender.output(SystemPromptOutput::Updated(
                    self.conversation_id.clone(),
                    prompt,
                ));
                root.close();
            }
            SystemPromptMsg::Cancel => {
                let _ = sender.output(SystemPromptOutput::Cancelled);
                root.close();
            }
            SystemPromptMsg::Clear => {
                self.buffer.set_text("");
            }
        }
    }
}
