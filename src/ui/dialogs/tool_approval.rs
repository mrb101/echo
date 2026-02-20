use adw::prelude::*;
use relm4::prelude::*;

use crate::providers::types::ToolCall;
use crate::services::agent::ApprovalDecision;

pub struct ToolApprovalDialog {
    tool_call: ToolCall,
}

pub struct ToolApprovalInit {
    pub tool_call: ToolCall,
}

#[derive(Debug)]
pub enum ToolApprovalMsg {
    Allow,
    Deny,
    AllowAlways,
}

#[derive(Debug)]
pub enum ToolApprovalOutput {
    Decision(ApprovalDecision),
}

impl ToolApprovalDialog {
    fn format_arguments(&self) -> String {
        if self.tool_call.arguments.is_null() {
            return "No arguments".to_string();
        }
        if let Some(obj) = self.tool_call.arguments.as_object() {
            if obj.is_empty() {
                return "No arguments".to_string();
            }
        }
        serde_json::to_string_pretty(&self.tool_call.arguments)
            .unwrap_or_else(|_| format!("{}", self.tool_call.arguments))
    }
}

#[relm4::component(pub, async)]
impl AsyncComponent for ToolApprovalDialog {
    type Init = ToolApprovalInit;
    type Input = ToolApprovalMsg;
    type Output = ToolApprovalOutput;
    type CommandOutput = ();

    view! {
        adw::Window {
            set_title: Some("Tool Approval Required"),
            set_default_width: 500,
            set_default_height: 400,
            set_modal: true,

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {},

                #[wrap(Some)]
                set_content = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_top: 16,
                    set_margin_bottom: 16,
                    set_margin_start: 16,
                    set_margin_end: 16,

                    gtk::Label {
                        set_label: "The AI wants to use a tool that requires your approval:",
                        set_wrap: true,
                        set_halign: gtk::Align::Start,
                    },

                    adw::PreferencesGroup {
                        set_title: "Tool Details",

                        adw::ActionRow {
                            set_title: "Tool",
                            set_subtitle: &model.tool_call.name,
                        },
                    },

                    gtk::Label {
                        set_label: "Arguments",
                        set_halign: gtk::Align::Start,
                        add_css_class: "heading",
                    },

                    #[name = "args_scroll"]
                    gtk::ScrolledWindow {
                        set_vexpand: true,
                        set_min_content_height: 80,

                        #[name = "args_view"]
                        gtk::TextView {
                            set_editable: false,
                            set_cursor_visible: false,
                            set_monospace: true,
                            set_wrap_mode: gtk::WrapMode::WordChar,
                            set_top_margin: 8,
                            set_bottom_margin: 8,
                            set_left_margin: 8,
                            set_right_margin: 8,
                            add_css_class: "card",
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_halign: gtk::Align::End,

                        gtk::Button {
                            set_label: "Deny",
                            add_css_class: "destructive-action",
                            connect_clicked => ToolApprovalMsg::Deny,
                        },

                        gtk::Button {
                            set_label: "Allow Once",
                            add_css_class: "suggested-action",
                            connect_clicked => ToolApprovalMsg::Allow,
                        },

                        gtk::Button {
                            set_label: "Always Allow",
                            connect_clicked => ToolApprovalMsg::AllowAlways,
                        },
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
        let model = Self {
            tool_call: init.tool_call,
        };
        let widgets = view_output!();

        // Set arguments text in the TextView
        let args_text = model.format_arguments();
        widgets.args_view.buffer().set_text(&args_text);

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        let decision = match msg {
            ToolApprovalMsg::Allow => ApprovalDecision::Allow,
            ToolApprovalMsg::Deny => ApprovalDecision::Deny,
            ToolApprovalMsg::AllowAlways => ApprovalDecision::AllowAlways,
        };
        let _ = sender.output(ToolApprovalOutput::Decision(decision));
        root.close();
    }
}
