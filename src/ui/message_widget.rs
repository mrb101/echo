use gtk::prelude::*;
use relm4::prelude::*;

use crate::models::{Message, Role};
use crate::services::markdown::{parse_markdown, spans_to_pango_markup, MessageBlock};

/// Wrapper struct for MessageWidget initialization.
pub struct MessageWidgetInit {
    pub message: Message,
    pub show_date_separator: Option<String>, // e.g. "January 15, 2025"
}

pub struct MessageWidget {
    pub message: Message,
    show_date_separator: Option<String>,
    content_box: gtk::Box,
    bubble: gtk::Box,
    action_bar: gtk::Box,
    outer_box: gtk::Box, // outermost container (includes date separator)
    message_row: Option<gtk::Box>,
    is_user: bool,
    // Edit mode state
    editing: bool,
    edit_buffer: Option<gtk::TextBuffer>,
    edit_container: Option<gtk::Box>,
    original_content_visible: bool,
}

#[derive(Debug)]
pub enum MessageWidgetMsg {
    UpdateContent(String),
    StreamingComplete,
    SetTokens(Option<i64>, Option<i64>),
    // Edit
    StartEdit,
    SaveEdit,
    CancelEdit,
    // Copy
    RequestCopy,
    // Search
    SetSearchHighlight(Option<String>),
    // Responsive sizing
    SetMaxWidth(i32),
}

#[derive(Debug)]
pub enum MessageWidgetOutput {
    Regenerate(String),          // message_id
    EditMessage(String, String), // message_id, new_content
    CopyFullContent(String),     // content
}

#[relm4::factory(pub)]
impl FactoryComponent for MessageWidget {
    type Init = MessageWidgetInit;
    type Input = MessageWidgetMsg;
    type Output = MessageWidgetOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin_start(8)
            .margin_end(8)
            .margin_top(8)
            .margin_bottom(8)
            .build();

        let bubble = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();

        let action_bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .margin_top(2)
            .margin_end(4)
            .visible(false)
            .build();
        action_bar.add_css_class("message-actions");

        let outer_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let is_user = init.message.role == Role::User;
        Self {
            message: init.message,
            show_date_separator: init.show_date_separator,
            content_box,
            bubble,
            action_bar,
            outer_box,
            message_row: None,
            is_user,
            editing: false,
            edit_buffer: None,
            edit_container: None,
            original_content_visible: true,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let is_user = self.message.role == Role::User;

        // Date separator
        if let Some(date_text) = &self.show_date_separator {
            let sep_label = gtk::Label::builder()
                .label(date_text)
                .halign(gtk::Align::Center)
                .margin_top(12)
                .margin_bottom(8)
                .build();
            sep_label.add_css_class("dim-label");
            sep_label.add_css_class("caption");
            sep_label.add_css_class("date-separator");
            self.outer_box.append(&sep_label);
        }

        if is_user {
            self.bubble.add_css_class("message-bubble-user");
        } else {
            self.bubble.add_css_class("message-bubble-assistant");
        }
        self.bubble.add_css_class("card");

        // Role label + timestamp in a horizontal box
        let role_time_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_start(8)
            .margin_end(8)
            .margin_top(4)
            .build();

        let role_label = gtk::Label::builder()
            .label(match self.message.role {
                Role::User => "You",
                Role::Assistant => self.message.model.as_deref().unwrap_or("Assistant"),
            })
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        role_label.add_css_class("caption");
        role_label.add_css_class("dim-label");
        role_time_box.append(&role_label);

        let time_label = gtk::Label::builder()
            .label(self.message.created_at.format("%H:%M").to_string())
            .halign(gtk::Align::End)
            .build();
        time_label.add_css_class("caption");
        time_label.add_css_class("dim-label");
        time_label.add_css_class("message-timestamp");
        role_time_box.append(&time_label);

        self.bubble.append(&role_time_box);

        if is_user {
            // Render attachment thumbnails for user messages
            if !self.message.attachments.is_empty() {
                let images_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(4)
                    .build();
                for att in &self.message.attachments {
                    let bytes = glib::Bytes::from(&att.data);
                    if let Ok(texture) = gtk::gdk::Texture::from_bytes(&bytes) {
                        let image = gtk::Image::from_paintable(Some(&texture));
                        image.set_pixel_size(60);
                        image.add_css_class("attachment-thumbnail");
                        images_box.append(&image);
                    }
                }
                self.content_box.append(&images_box);
            }

            let label = gtk::Label::builder()
                .label(&self.message.content)
                .halign(gtk::Align::Start)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .selectable(true)
                .build();
            self.content_box.append(&label);
        } else {
            render_markdown_blocks(&self.content_box, &self.message.content);
        }

        self.bubble.append(&self.content_box);

        // Token info for assistant messages
        if !is_user {
            if let (Some(ti), Some(to)) = (self.message.tokens_in, self.message.tokens_out) {
                let token_label = gtk::Label::builder()
                    .label(format!("\u{2193}{} \u{2191}{} tokens", ti, to))
                    .halign(gtk::Align::End)
                    .margin_end(8)
                    .margin_bottom(2)
                    .build();
                token_label.add_css_class("token-info");
                token_label.add_css_class("dim-label");
                token_label.add_css_class("caption");
                self.bubble.append(&token_label);
            }
        }

        // Wrap bubble in overlay for action buttons
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&self.bubble));

        // Build action buttons
        let copy_btn = gtk::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text("Copy message")
            .build();
        copy_btn.add_css_class("flat");
        copy_btn.add_css_class("circular");
        let sender_copy = sender.input_sender().clone();
        copy_btn.connect_clicked(move |_| {
            sender_copy.send(MessageWidgetMsg::RequestCopy).unwrap();
        });
        self.action_bar.append(&copy_btn);

        if !is_user {
            // Regenerate button for assistant messages
            let msg_id = self.message.id.clone();
            let regen_btn = gtk::Button::builder()
                .icon_name("view-refresh-symbolic")
                .tooltip_text("Regenerate")
                .build();
            regen_btn.add_css_class("flat");
            regen_btn.add_css_class("circular");
            let sender_regen = sender.output_sender().clone();
            regen_btn.connect_clicked(move |_| {
                let _ = sender_regen.send(MessageWidgetOutput::Regenerate(msg_id.clone()));
            });
            self.action_bar.append(&regen_btn);
        } else {
            // Edit button for user messages
            let edit_btn = gtk::Button::builder()
                .icon_name("document-edit-symbolic")
                .tooltip_text("Edit message")
                .build();
            edit_btn.add_css_class("flat");
            edit_btn.add_css_class("circular");
            let sender_edit = sender.input_sender().clone();
            edit_btn.connect_clicked(move |_| {
                sender_edit.send(MessageWidgetMsg::StartEdit).unwrap();
            });
            self.action_bar.append(&edit_btn);
        }

        overlay.add_overlay(&self.action_bar);
        self.action_bar.set_halign(gtk::Align::End);
        self.action_bar.set_valign(gtk::Align::Start);

        // Show/hide on hover
        let action_bar_ref = self.action_bar.clone();
        let motion = gtk::EventControllerMotion::new();
        let action_bar_enter = action_bar_ref.clone();
        motion.connect_enter(move |_, _, _| {
            action_bar_enter.set_visible(true);
        });
        let action_bar_leave = action_bar_ref;
        motion.connect_leave(move |_| {
            action_bar_leave.set_visible(false);
        });
        overlay.add_controller(motion);

        // Build the message row (with alignment)
        let message_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(12)
            .margin_end(12)
            .halign(if is_user {
                gtk::Align::End
            } else {
                gtk::Align::Start
            })
            .build();
        message_row.append(&overlay);
        self.message_row = Some(message_row.clone());

        self.outer_box.append(&message_row);
        root.append(&self.outer_box);

        let widgets = view_output!();
        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            MessageWidgetMsg::UpdateContent(text) => {
                self.message.content = text.clone();
                render_markdown_blocks(&self.content_box, &text);
            }
            MessageWidgetMsg::StreamingComplete => {
                // Final re-render already done via UpdateContent
            }
            MessageWidgetMsg::SetTokens(tokens_in, tokens_out) => {
                self.message.tokens_in = tokens_in;
                self.message.tokens_out = tokens_out;
                if let (Some(ti), Some(to)) = (tokens_in, tokens_out) {
                    let token_label = gtk::Label::builder()
                        .label(format!("\u{2193}{} \u{2191}{} tokens", ti, to))
                        .halign(gtk::Align::End)
                        .margin_end(8)
                        .margin_bottom(2)
                        .build();
                    token_label.add_css_class("token-info");
                    token_label.add_css_class("dim-label");
                    token_label.add_css_class("caption");
                    self.bubble.append(&token_label);
                }
            }
            MessageWidgetMsg::StartEdit => {
                if self.editing {
                    return;
                }
                self.editing = true;
                self.content_box.set_visible(false);
                self.original_content_visible = false;

                let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
                buffer.set_text(&self.message.content);

                let text_view = gtk::TextView::builder()
                    .buffer(&buffer)
                    .wrap_mode(gtk::WrapMode::WordChar)
                    .top_margin(8)
                    .bottom_margin(8)
                    .left_margin(8)
                    .right_margin(8)
                    .height_request(80)
                    .build();
                text_view.add_css_class("card");

                let btn_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(8)
                    .halign(gtk::Align::End)
                    .build();

                let cancel_btn = gtk::Button::builder().label("Cancel").build();
                let sender_cancel = sender.input_sender().clone();
                cancel_btn.connect_clicked(move |_| {
                    sender_cancel.send(MessageWidgetMsg::CancelEdit).unwrap();
                });
                btn_box.append(&cancel_btn);

                let save_btn = gtk::Button::builder().label("Save & Resend").build();
                save_btn.add_css_class("suggested-action");
                let sender_save = sender.input_sender().clone();
                save_btn.connect_clicked(move |_| {
                    sender_save.send(MessageWidgetMsg::SaveEdit).unwrap();
                });
                btn_box.append(&save_btn);

                let edit_container = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .margin_start(8)
                    .margin_end(8)
                    .margin_top(8)
                    .margin_bottom(8)
                    .build();
                edit_container.append(&text_view);
                edit_container.append(&btn_box);

                self.bubble.append(&edit_container);
                self.edit_buffer = Some(buffer);
                self.edit_container = Some(edit_container);
            }
            MessageWidgetMsg::SaveEdit => {
                if let Some(buffer) = &self.edit_buffer {
                    let start = buffer.start_iter();
                    let end = buffer.end_iter();
                    let new_text = buffer.text(&start, &end, false).to_string();

                    if !new_text.trim().is_empty() {
                        let _ = sender.output(MessageWidgetOutput::EditMessage(
                            self.message.id.clone(),
                            new_text,
                        ));
                    }
                }
                self.cleanup_edit();
            }
            MessageWidgetMsg::CancelEdit => {
                self.cleanup_edit();
            }
            MessageWidgetMsg::RequestCopy => {
                let _ = sender.output(MessageWidgetOutput::CopyFullContent(
                    self.message.content.clone(),
                ));
            }
            MessageWidgetMsg::SetSearchHighlight(term) => {
                if let Some(ref term) = term {
                    let matches = self.message.content.to_lowercase().contains(term.as_str());
                    self.outer_box.set_opacity(if matches { 1.0 } else { 0.3 });
                } else {
                    self.outer_box.set_opacity(1.0);
                }
            }
            MessageWidgetMsg::SetMaxWidth(width) => {
                if let Some(ref row) = self.message_row {
                    if self.is_user {
                        row.set_margin_start(12_i32.max(width * 25 / 100));
                        row.set_margin_end(12);
                    } else {
                        row.set_margin_start(12);
                        row.set_margin_end(12_i32.max(width * 15 / 100));
                    }
                }
            }
        }
    }
}

impl MessageWidget {
    fn cleanup_edit(&mut self) {
        self.editing = false;
        if let Some(container) = self.edit_container.take() {
            self.bubble.remove(&container);
        }
        self.edit_buffer = None;
        self.content_box.set_visible(true);
        self.original_content_visible = true;
    }
}

fn render_markdown_blocks(content_box: &gtk::Box, text: &str) {
    while let Some(child) = content_box.first_child() {
        content_box.remove(&child);
    }

    if text.is_empty() {
        return;
    }

    let blocks = parse_markdown(text);

    for block in &blocks {
        let widget = block_to_widget(block);
        content_box.append(&widget);
    }
}

fn block_to_widget(block: &MessageBlock) -> gtk::Widget {
    match block {
        MessageBlock::RichText(spans) => {
            let markup = spans_to_pango_markup(spans);
            let label = gtk::Label::builder()
                .halign(gtk::Align::Start)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .selectable(true)
                .use_markup(true)
                .build();
            label.set_markup(&markup);
            label.upcast()
        }
        MessageBlock::CodeBlock { language, code } => build_code_block(language.as_deref(), code),
        MessageBlock::Heading { level, spans } => {
            let markup = spans_to_pango_markup(spans);
            let label = gtk::Label::builder()
                .halign(gtk::Align::Start)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .selectable(true)
                .use_markup(true)
                .build();
            label.set_markup(&markup);
            let css_class = match level {
                1 => "heading-1",
                2 => "heading-2",
                3 => "heading-3",
                _ => "heading-4",
            };
            label.add_css_class(css_class);
            label.upcast()
        }
        MessageBlock::BlockQuote(inner_blocks) => {
            let bq_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .build();
            bq_box.add_css_class("blockquote");

            for inner_block in inner_blocks {
                let widget = block_to_widget(inner_block);
                bq_box.append(&widget);
            }
            bq_box.upcast()
        }
        MessageBlock::UnorderedList(items) => build_list(items, false),
        MessageBlock::OrderedList(items) => build_list(items, true),
        MessageBlock::HorizontalRule => {
            let sep = gtk::Separator::builder()
                .orientation(gtk::Orientation::Horizontal)
                .margin_top(4)
                .margin_bottom(4)
                .build();
            sep.upcast()
        }
    }
}

fn build_code_block(language: Option<&str>, code: &str) -> gtk::Widget {
    let outer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .margin_top(4)
        .margin_bottom(4)
        .build();
    outer.add_css_class("code-block");

    // Header with language label and copy button
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    header.add_css_class("code-block-header");

    let lang_label = gtk::Label::builder()
        .label(language.unwrap_or(""))
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();
    lang_label.add_css_class("code-block-language");
    header.append(&lang_label);

    let copy_button = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Copy code")
        .build();
    copy_button.add_css_class("flat");
    copy_button.add_css_class("circular");

    let code_for_copy = code.to_string();
    copy_button.connect_clicked(move |btn| {
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&code_for_copy);
            btn.set_icon_name("object-select-symbolic");
            let btn_clone = btn.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                // Guard: only update if button is still in a live widget tree.
                // During streaming re-renders or conversation switches, the code
                // block may be removed, orphaning this button reference.
                if btn_clone.parent().is_some() {
                    btn_clone.set_icon_name("edit-copy-symbolic");
                }
            });
        }
    });
    header.append(&copy_button);

    outer.append(&header);

    // Code content
    let text_view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .monospace(true)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(12)
        .right_margin(12)
        .build();
    text_view.buffer().set_text(code);
    text_view.add_css_class("code-block-content");

    outer.append(&text_view);

    outer.upcast()
}

fn build_list(items: &[Vec<MessageBlock>], ordered: bool) -> gtk::Widget {
    let list_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_start(4)
        .build();

    for (i, item_blocks) in items.iter().enumerate() {
        let item_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();

        let bullet_text = if ordered {
            format!("{}.", i + 1)
        } else {
            "\u{2022}".to_string()
        };

        let bullet = gtk::Label::builder()
            .label(&bullet_text)
            .valign(gtk::Align::Start)
            .build();
        bullet.add_css_class("list-bullet");
        item_row.append(&bullet);

        let item_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .build();

        for block in item_blocks {
            let widget = block_to_widget(block);
            item_content.append(&widget);
        }

        item_row.append(&item_content);
        list_box.append(&item_row);
    }

    list_box.upcast()
}
