use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::prelude::*;

use crate::models::Message;
use std::path::PathBuf;

use crate::providers::ImageAttachment;
use crate::ui::input_area::{InputArea, InputAreaMsg, InputAreaOutput};
use crate::ui::message_widget::{
    MessageWidget, MessageWidgetInit, MessageWidgetMsg, MessageWidgetOutput,
};

pub struct ChatView {
    messages: FactoryVecDeque<MessageWidget>,
    input_area: Controller<InputArea>,
    loading: bool,
    scrolled_window: gtk::ScrolledWindow,
    thinking_label: gtk::Label,
    // Streaming state
    streaming_message_id: Option<String>,
    streaming_buffer: Rc<RefCell<Option<StreamBuffer>>>,
    render_timer_active: Rc<RefCell<bool>>,
    // Auto-scroll
    user_scrolled_up: bool,
    // Search
    search_active: bool,
    search_term: String,
    // Track last message date for date separator logic
    last_message_date: Option<String>,
    // Responsive sizing
    container_width: i32,
}

struct StreamBuffer {
    message_id: String,
    accumulated_text: String,
    needs_render: bool,
}

#[derive(Debug)]
pub enum ChatViewMsg {
    AddMessage(Message),
    LoadMessages(Vec<Message>),
    Clear,
    SetLoading(bool),
    ScrollToBottom,
    UserSendMessage(String, Vec<ImageAttachment>),
    // Streaming
    AddStreamingMessage(Message),
    UpdateStreamingMessage(String, String), // (message_id, full_text)
    StreamingComplete(String),              // message_id
    RemoveMessage(String),                  // message_id (on error)
    StopGeneration,
    // Internal
    RenderBuffered,
    ScrollPositionChanged,
    // Forwarded from MessageWidget
    ForwardRegenerate(String),          // message_id
    ForwardEditMessage(String, String), // message_id, new_content
    CopyToClipboard(String),
    // Drag-and-drop
    ImageDropped(PathBuf),
    // Tokens
    SetMessageTokens(String, Option<i64>, Option<i64>), // message_id, tokens_in, tokens_out
    // Search
    ToggleSearch,
    SearchInConversation(String),
    // Responsive sizing
    ContainerWidthChanged(i32),
    // Agent tool activity
    ShowToolActivity {
        tool_name: String,
        call_id: String,
    },
    UpdateToolResult {
        tool_name: String,
        duration_ms: u64,
        is_error: bool,
    },
}

#[derive(Debug)]
pub enum ChatViewOutput {
    SendMessage {
        text: String,
        images: Vec<ImageAttachment>,
    },
    StopGeneration,
    RegenerateMessage(String),   // message_id
    EditMessage(String, String), // message_id, new_content
}

#[relm4::component(pub)]
impl Component for ChatView {
    type Init = ();
    type Input = ChatViewMsg;
    type Output = ChatViewOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_vexpand: true,

            // Search bar
            #[name = "search_bar"]
            gtk::SearchBar {
                #[watch]
                set_search_mode: model.search_active,
            },

            // Overlay: scrolled area + scroll-to-bottom button
            gtk::Overlay {
                set_vexpand: true,

                // Scrollable message area
                #[local_ref]
                scrolled_window -> gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hscrollbar_policy: gtk::PolicyType::Never,

                    #[local_ref]
                    message_list -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 0,
                        set_margin_top: 8,
                        set_margin_bottom: 8,
                        set_margin_start: 16,
                        set_margin_end: 16,
                    },
                },

                // Scroll-to-bottom floating button
                add_overlay = &gtk::Button {
                    set_icon_name: "go-down-symbolic",
                    set_tooltip_text: Some("Scroll to bottom"),
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::End,
                    add_css_class: "circular",
                    add_css_class: "osd",
                    add_css_class: "scroll-to-bottom",
                    #[watch]
                    set_visible: model.user_scrolled_up,
                    connect_clicked => ChatViewMsg::ScrollToBottom,
                },
            },

            // Loading / stop generation area
            #[local_ref]
            loading_box -> gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_halign: gtk::Align::Start,
                set_margin_start: 20,
                set_margin_bottom: 8,
                set_spacing: 8,
                #[watch]
                set_visible: model.loading,
            },

            // Separator
            gtk::Separator {
                set_orientation: gtk::Orientation::Horizontal,
            },

            // Input area
            model.input_area.widget().clone(),
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let messages = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                MessageWidgetOutput::Regenerate(msg_id) => ChatViewMsg::ForwardRegenerate(msg_id),
                MessageWidgetOutput::EditMessage(msg_id, new_content) => {
                    ChatViewMsg::ForwardEditMessage(msg_id, new_content)
                }
                MessageWidgetOutput::CopyFullContent(content) => {
                    ChatViewMsg::CopyToClipboard(content)
                }
            });

        let input_area = InputArea::builder()
            .launch(())
            .forward(sender.input_sender(), |output| match output {
                InputAreaOutput::SendMessage { text, images } => {
                    ChatViewMsg::UserSendMessage(text, images)
                }
            });

        let scrolled_window = gtk::ScrolledWindow::new();

        // Build loading box with stop button
        let loading_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let spinner = gtk::Spinner::builder().spinning(true).build();
        loading_box.append(&spinner);

        let thinking_label = gtk::Label::builder().label("Generating...").build();
        thinking_label.add_css_class("dim-label");
        loading_box.append(&thinking_label);

        let stop_btn = gtk::Button::builder()
            .label("Stop")
            .tooltip_text("Stop generating")
            .build();
        stop_btn.add_css_class("destructive-action");
        stop_btn.add_css_class("pill");
        let sender_stop = sender.input_sender().clone();
        stop_btn.connect_clicked(move |_| {
            sender_stop.send(ChatViewMsg::StopGeneration).unwrap();
        });
        loading_box.append(&stop_btn);

        let model = Self {
            messages,
            input_area,
            loading: false,
            scrolled_window: scrolled_window.clone(),
            thinking_label: thinking_label.clone(),
            streaming_message_id: None,
            streaming_buffer: Rc::new(RefCell::new(None)),
            render_timer_active: Rc::new(RefCell::new(false)),
            user_scrolled_up: false,
            search_active: false,
            search_term: String::new(),
            last_message_date: None,
            container_width: 0,
        };

        let message_list = model.messages.widget();
        let widgets = view_output!();

        // Add search entry to search bar imperatively
        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Search in conversation...")
            .hexpand(true)
            .build();
        widgets.search_bar.set_child(Some(&search_entry));
        widgets.search_bar.connect_entry(&search_entry);

        // Connect vadjustment to track scroll position
        let sender_scroll = sender.input_sender().clone();
        scrolled_window
            .vadjustment()
            .connect_value_changed(move |_| {
                sender_scroll
                    .send(ChatViewMsg::ScrollPositionChanged)
                    .unwrap();
            });

        // Track container width for responsive bubble sizing
        let sender_resize = sender.input_sender().clone();
        let last_width: Rc<RefCell<i32>> = Rc::new(RefCell::new(0));
        let last_width_clone = last_width.clone();
        scrolled_window.add_tick_callback(move |widget, _| {
            let w = widget.width();
            if w > 0 && w != *last_width_clone.borrow() {
                *last_width_clone.borrow_mut() = w;
                let _ = sender_resize.send(ChatViewMsg::ContainerWidthChanged(w));
            }
            glib::ControlFlow::Continue
        });

        // Connect search entry
        let sender_search = sender.input_sender().clone();
        search_entry.connect_search_changed(move |entry| {
            sender_search
                .send(ChatViewMsg::SearchInConversation(entry.text().to_string()))
                .unwrap();
        });

        // Escape closes search bar
        let sender_esc = sender.input_sender().clone();
        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                sender_esc.send(ChatViewMsg::ToggleSearch).unwrap();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        search_entry.add_controller(key_ctrl);

        // Set up drag-and-drop for images
        let drop_target =
            gtk::DropTarget::new(gio::File::static_type(), gtk::gdk::DragAction::COPY);
        let root_ref = root.clone();
        let sender_drop = sender.input_sender().clone();
        drop_target.connect_drop(move |_, value, _, _| {
            if let Ok(file) = value.get::<gio::File>() {
                if let Some(path) = file.path() {
                    // Check if it's an image
                    let is_image = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|ext| {
                            matches!(
                                ext.to_lowercase().as_str(),
                                "png" | "jpg" | "jpeg" | "gif" | "webp"
                            )
                        });
                    if is_image {
                        sender_drop.send(ChatViewMsg::ImageDropped(path)).unwrap();
                        return true;
                    }
                }
            }
            false
        });

        // Visual hover indicator
        let root_enter = root_ref.clone();
        drop_target.connect_enter(move |_, _, _| {
            root_enter.add_css_class("drag-hover");
            gtk::gdk::DragAction::COPY
        });
        let root_leave = root_ref;
        drop_target.connect_leave(move |_| {
            root_leave.remove_css_class("drag-hover");
        });

        root.add_controller(drop_target);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            ChatViewMsg::AddMessage(message) => {
                let date_sep = self.compute_date_separator(&message);
                let mut guard = self.messages.guard();
                guard.push_back(MessageWidgetInit {
                    message,
                    show_date_separator: date_sep,
                });
                let last_idx = guard.len() - 1;
                if self.container_width > 0 {
                    guard.send(
                        last_idx,
                        MessageWidgetMsg::SetMaxWidth(self.container_width),
                    );
                }
                drop(guard);
                self.auto_scroll_to_bottom(&sender);
            }
            ChatViewMsg::LoadMessages(messages) => {
                let mut guard = self.messages.guard();
                guard.clear();
                self.last_message_date = None;

                for (i, msg) in messages.iter().enumerate() {
                    let date_sep = if i == 0 {
                        Some(msg.created_at.format("%B %e, %Y").to_string())
                    } else {
                        let prev = &messages[i - 1];
                        if msg.created_at.date_naive() != prev.created_at.date_naive() {
                            Some(msg.created_at.format("%B %e, %Y").to_string())
                        } else {
                            None
                        }
                    };
                    self.last_message_date = Some(msg.created_at.date_naive().to_string());
                    guard.push_back(MessageWidgetInit {
                        message: msg.clone(),
                        show_date_separator: date_sep,
                    });
                }
                if self.container_width > 0 {
                    for i in 0..guard.len() {
                        guard.send(i, MessageWidgetMsg::SetMaxWidth(self.container_width));
                    }
                }
                drop(guard);
                sender.input(ChatViewMsg::ScrollToBottom);
            }
            ChatViewMsg::Clear => {
                let mut guard = self.messages.guard();
                guard.clear();
                self.streaming_message_id = None;
                *self.streaming_buffer.borrow_mut() = None;
                self.last_message_date = None;
            }
            ChatViewMsg::SetLoading(loading) => {
                self.loading = loading;
                self.input_area.emit(InputAreaMsg::SetSending(loading));
                if loading {
                    self.thinking_label.set_label("Generating...");
                }
            }
            ChatViewMsg::ScrollToBottom => {
                self.user_scrolled_up = false;
                let adj = self.scrolled_window.vadjustment();
                glib::idle_add_local_once(move || {
                    adj.set_value(adj.upper());
                });
            }
            ChatViewMsg::ScrollPositionChanged => {
                let adj = self.scrolled_window.vadjustment();
                let at_bottom = adj.value() >= adj.upper() - adj.page_size() - 50.0;
                self.user_scrolled_up = !at_bottom;
            }
            ChatViewMsg::UserSendMessage(text, images) => {
                let _ = sender.output(ChatViewOutput::SendMessage { text, images });
            }
            // Streaming messages
            ChatViewMsg::AddStreamingMessage(message) => {
                self.streaming_message_id = Some(message.id.clone());
                let date_sep = self.compute_date_separator(&message);
                let mut guard = self.messages.guard();
                guard.push_back(MessageWidgetInit {
                    message,
                    show_date_separator: date_sep,
                });
                let last_idx = guard.len() - 1;
                if self.container_width > 0 {
                    guard.send(
                        last_idx,
                        MessageWidgetMsg::SetMaxWidth(self.container_width),
                    );
                }
                drop(guard);
                self.auto_scroll_to_bottom(&sender);
            }
            ChatViewMsg::UpdateStreamingMessage(message_id, full_text) => {
                // Buffer the update for timer-based rendering
                let mut buf = self.streaming_buffer.borrow_mut();
                *buf = Some(StreamBuffer {
                    message_id,
                    accumulated_text: full_text,
                    needs_render: true,
                });
                drop(buf);

                // Start render timer if not active
                if !*self.render_timer_active.borrow() {
                    *self.render_timer_active.borrow_mut() = true;
                    let sender_timer = sender.input_sender().clone();
                    let timer_active = self.render_timer_active.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
                        sender_timer.send(ChatViewMsg::RenderBuffered).unwrap();
                        if *timer_active.borrow() {
                            glib::ControlFlow::Continue
                        } else {
                            glib::ControlFlow::Break
                        }
                    });
                }
            }
            ChatViewMsg::RenderBuffered => {
                let mut buf = self.streaming_buffer.borrow_mut();
                if let Some(buffer) = buf.as_mut() {
                    if buffer.needs_render {
                        buffer.needs_render = false;
                        let text = buffer.accumulated_text.clone();
                        let msg_id = buffer.message_id.clone();
                        drop(buf);

                        // Find and update the streaming message
                        self.update_message_content(&msg_id, &text);
                        self.auto_scroll_to_bottom(&sender);
                    }
                }
            }
            ChatViewMsg::StreamingComplete(message_id) => {
                // Stop the render timer
                *self.render_timer_active.borrow_mut() = false;

                // Do a final render from the buffer
                let final_text = {
                    let buf = self.streaming_buffer.borrow();
                    buf.as_ref().map(|b| b.accumulated_text.clone())
                };
                if let Some(text) = final_text {
                    self.update_message_content(&message_id, &text);
                }

                // Send streaming complete to the widget
                self.send_to_streaming_widget(&message_id, MessageWidgetMsg::StreamingComplete);

                self.streaming_message_id = None;
                *self.streaming_buffer.borrow_mut() = None;

                // Reset thinking label for next use
                self.thinking_label.set_label("Generating...");
            }
            ChatViewMsg::RemoveMessage(message_id) => {
                // Stop render timer
                *self.render_timer_active.borrow_mut() = false;
                *self.streaming_buffer.borrow_mut() = None;

                let guard = self.messages.guard();
                let pos = guard.iter().position(|m| m.message.id == message_id);
                drop(guard);

                if let Some(pos) = pos {
                    let mut guard = self.messages.guard();
                    guard.remove(pos);
                }

                self.streaming_message_id = None;
            }
            ChatViewMsg::StopGeneration => {
                let _ = sender.output(ChatViewOutput::StopGeneration);
            }
            ChatViewMsg::ForwardRegenerate(msg_id) => {
                let _ = sender.output(ChatViewOutput::RegenerateMessage(msg_id));
            }
            ChatViewMsg::ForwardEditMessage(msg_id, new_content) => {
                let _ = sender.output(ChatViewOutput::EditMessage(msg_id, new_content));
            }
            ChatViewMsg::CopyToClipboard(content) => {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&content);
                }
            }
            ChatViewMsg::ImageDropped(path) => {
                self.input_area.emit(InputAreaMsg::AddImageFromPath(path));
            }
            ChatViewMsg::SetMessageTokens(message_id, tokens_in, tokens_out) => {
                let guard = self.messages.guard();
                let pos = guard.iter().position(|m| m.message.id == message_id);
                if let Some(idx) = pos {
                    guard.send(idx, MessageWidgetMsg::SetTokens(tokens_in, tokens_out));
                }
            }
            ChatViewMsg::ToggleSearch => {
                self.search_active = !self.search_active;
                if !self.search_active {
                    // Clear search when closing
                    self.search_term.clear();
                    self.clear_search_highlight();
                }
            }
            ChatViewMsg::SearchInConversation(term) => {
                self.search_term = term.to_lowercase();
                self.apply_search_highlight();
            }
            ChatViewMsg::ContainerWidthChanged(width) => {
                if self.container_width != width {
                    self.container_width = width;
                    let guard = self.messages.guard();
                    for i in 0..guard.len() {
                        guard.send(i, MessageWidgetMsg::SetMaxWidth(width));
                    }
                }
            }
            ChatViewMsg::ShowToolActivity { tool_name, call_id: _ } => {
                self.thinking_label
                    .set_label(&format!("Running tool: {}...", tool_name));
            }
            ChatViewMsg::UpdateToolResult {
                tool_name,
                duration_ms,
                is_error,
            } => {
                if is_error {
                    self.thinking_label
                        .set_label(&format!("Tool {} failed", tool_name));
                } else {
                    self.thinking_label
                        .set_label(&format!("Tool {} done ({}ms)", tool_name, duration_ms));
                }
            }
        }
    }
}

impl ChatView {
    fn auto_scroll_to_bottom(&mut self, sender: &ComponentSender<Self>) {
        let adj = self.scrolled_window.vadjustment();
        let at_bottom = adj.value() >= adj.upper() - adj.page_size() - 50.0;
        self.user_scrolled_up = !at_bottom;

        if !self.user_scrolled_up {
            sender.input(ChatViewMsg::ScrollToBottom);
        }
    }

    fn update_message_content(&mut self, message_id: &str, text: &str) {
        let guard = self.messages.guard();
        let pos = guard.iter().position(|m| m.message.id == *message_id);
        if let Some(idx) = pos {
            guard.send(idx, MessageWidgetMsg::UpdateContent(text.to_string()));
        }
    }

    fn send_to_streaming_widget(&mut self, message_id: &str, msg: MessageWidgetMsg) {
        let guard = self.messages.guard();
        let pos = guard.iter().position(|m| m.message.id == *message_id);
        if let Some(idx) = pos {
            guard.send(idx, msg);
        }
    }

    fn compute_date_separator(&mut self, message: &Message) -> Option<String> {
        let msg_date = message.created_at.date_naive().to_string();
        let needs_sep = self.last_message_date.as_deref() != Some(&msg_date);
        self.last_message_date = Some(msg_date);
        if needs_sep {
            Some(message.created_at.format("%B %e, %Y").to_string())
        } else {
            None
        }
    }

    fn apply_search_highlight(&mut self) {
        let guard = self.messages.guard();
        if self.search_term.is_empty() {
            for i in 0..guard.len() {
                guard.send(i, MessageWidgetMsg::SetSearchHighlight(None));
            }
        } else {
            for i in 0..guard.len() {
                guard.send(
                    i,
                    MessageWidgetMsg::SetSearchHighlight(Some(self.search_term.clone())),
                );
            }
        }
    }

    fn clear_search_highlight(&mut self) {
        let guard = self.messages.guard();
        for i in 0..guard.len() {
            guard.send(i, MessageWidgetMsg::SetSearchHighlight(None));
        }
    }
}
