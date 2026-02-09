use std::path::PathBuf;

use gtk::prelude::*;
use relm4::prelude::*;

use crate::providers::ImageAttachment;

pub struct PendingImage {
    pub mime_type: String,
    pub data: Vec<u8>,
    pub container: gtk::Box,
}

pub struct InputArea {
    buffer: gtk::TextBuffer,
    sending: bool,
    pending_images: Vec<PendingImage>,
    attachment_strip: gtk::FlowBox,
    char_count: i32,
}

#[derive(Debug)]
pub enum InputAreaMsg {
    SendClicked,
    SetSending(bool),
    AttachImage,
    AddImageFromPath(PathBuf),
    RemoveAttachment(usize),
    // Internal
    ImageFileSelected(PathBuf),
    TextChanged,
    PasteImage(Vec<u8>),
}

#[derive(Debug)]
pub enum InputAreaOutput {
    SendMessage {
        text: String,
        images: Vec<ImageAttachment>,
    },
}

#[relm4::component(pub)]
impl Component for InputArea {
    type Init = ();
    type Input = InputAreaMsg;
    type Output = InputAreaOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            // Attachment thumbnail strip
            #[local_ref]
            attachment_strip -> gtk::FlowBox {
                set_selection_mode: gtk::SelectionMode::None,
                set_homogeneous: false,
                set_min_children_per_line: 1,
                set_max_children_per_line: 6,
                set_halign: gtk::Align::Start,
                set_margin_start: 8,
                set_margin_end: 8,
                set_margin_top: 4,
                #[watch]
                set_visible: !model.pending_images.is_empty(),
                add_css_class: "attachment-strip",
            },

            // Input card
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_margin_top: 8,
                set_margin_bottom: 8,
                set_margin_start: 12,
                set_margin_end: 12,
                add_css_class: "input-card",

                // Text input with placeholder overlay
                gtk::Overlay {
                    set_hexpand: true,

                    #[name = "input_scroll"]
                    gtk::ScrolledWindow {
                        set_hexpand: true,
                        set_max_content_height: 150,
                        set_propagate_natural_height: true,
                        set_min_content_height: 40,

                        #[name = "text_view"]
                        gtk::TextView {
                            set_wrap_mode: gtk::WrapMode::WordChar,
                            set_accepts_tab: false,
                            set_top_margin: 8,
                            set_bottom_margin: 8,
                            set_left_margin: 8,
                            set_right_margin: 8,
                            add_css_class: "input-text-view",

                            set_buffer: Some(&model.buffer),
                        },
                    },

                    // Markdown hints placeholder
                    add_overlay = &gtk::Label {
                        set_label: "**bold** *italic* `code` — Shift+Enter for new line",
                        set_halign: gtk::Align::Start,
                        set_valign: gtk::Align::Start,
                        set_margin_start: 12,
                        set_margin_top: 8,
                        add_css_class: "input-placeholder",
                        #[watch]
                        set_visible: model.char_count == 0 && !model.sending,
                    },
                },

                // Bottom toolbar
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                    set_margin_start: 4,
                    set_margin_end: 4,
                    set_margin_bottom: 4,
                    add_css_class: "input-toolbar",

                    // Attach button
                    gtk::Button {
                        set_icon_name: "list-add-symbolic",
                        set_tooltip_text: Some("Attach image"),
                        set_halign: gtk::Align::Start,
                        add_css_class: "flat",
                        add_css_class: "circular",
                        connect_clicked => InputAreaMsg::AttachImage,
                    },

                    // Spacer
                    gtk::Box {
                        set_hexpand: true,
                    },

                    #[name = "send_button"]
                    gtk::Button {
                        set_icon_name: "go-up-symbolic",
                        set_tooltip_text: Some("Send message (Enter)"),
                        set_halign: gtk::Align::End,
                        add_css_class: "suggested-action",
                        add_css_class: "circular",
                        #[watch]
                        set_sensitive: !model.sending && (model.buffer.char_count() > 0 || !model.pending_images.is_empty()),
                        connect_clicked => InputAreaMsg::SendClicked,
                    },
                },
            },

            // Character count
            gtk::Label {
                set_halign: gtk::Align::End,
                set_margin_end: 24,
                set_margin_bottom: 2,
                add_css_class: "dim-label",
                add_css_class: "caption",
                #[watch]
                set_visible: model.char_count > 0,
                #[watch]
                set_label: &format!("{} characters", model.char_count),
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        let attachment_strip = gtk::FlowBox::new();

        let model = Self {
            buffer: buffer.clone(),
            sending: false,
            pending_images: Vec::new(),
            attachment_strip: attachment_strip.clone(),
            char_count: 0,
        };

        let widgets = view_output!();

        // Connect key press on text_view: Enter sends, Shift+Enter newline, Ctrl+V paste image
        let sender_key = sender.clone();
        let key_controller = gtk::EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _code, modifier| {
            if key == gtk::gdk::Key::Return && !modifier.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                sender_key.input(InputAreaMsg::SendClicked);
                gtk::glib::Propagation::Stop
            } else if key == gtk::gdk::Key::v && modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                // Try to paste image from clipboard
                let sender_paste = sender_key.input_sender().clone();
                if let Some(display) = gtk::gdk::Display::default() {
                    let clipboard = display.clipboard();
                    clipboard.read_texture_async(None::<&gio::Cancellable>, move |result| {
                        if let Ok(Some(texture)) = result {
                            let bytes = texture.save_to_png_bytes();
                            sender_paste.send(InputAreaMsg::PasteImage(bytes.to_vec())).unwrap();
                        }
                    });
                }
                // Don't stop propagation - let normal text paste happen too
                gtk::glib::Propagation::Proceed
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        widgets.text_view.add_controller(key_controller);

        // Watch buffer changes to update send button sensitivity and char count
        let sender_buf = sender.clone();
        buffer.connect_changed(move |_| {
            sender_buf.input(InputAreaMsg::TextChanged);
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            InputAreaMsg::SendClicked => {
                let text = self.get_text();
                let trimmed = text.trim().to_string();
                let has_images = !self.pending_images.is_empty();

                if (!trimmed.is_empty() || has_images) && !self.sending {
                    let images: Vec<ImageAttachment> = self
                        .pending_images
                        .drain(..)
                        .map(|pi| ImageAttachment {
                            mime_type: pi.mime_type,
                            data: pi.data,
                        })
                        .collect();

                    let _ = sender.output(InputAreaOutput::SendMessage {
                        text: trimmed,
                        images,
                    });
                    self.buffer.set_text("");
                    self.clear_attachment_strip();
                }
            }
            InputAreaMsg::SetSending(sending) => {
                self.sending = sending;
            }
            InputAreaMsg::AttachImage => {
                let dialog = gtk::FileDialog::builder()
                    .title("Attach Image")
                    .build();

                // Set image file filter
                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Images"));
                filter.add_mime_type("image/png");
                filter.add_mime_type("image/jpeg");
                filter.add_mime_type("image/gif");
                filter.add_mime_type("image/webp");
                let filters = gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));

                let sender_dlg = sender.input_sender().clone();
                if let Some(window) = root.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
                    dialog.open(Some(&window), None::<&gio::Cancellable>, move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file.path() {
                                sender_dlg.send(InputAreaMsg::ImageFileSelected(path)).unwrap();
                            }
                        }
                    });
                }
            }
            InputAreaMsg::TextChanged => {
                self.char_count = self.buffer.char_count();
            }
            InputAreaMsg::ImageFileSelected(path) | InputAreaMsg::AddImageFromPath(path) => {
                self.add_image_from_path(path, &sender);
            }
            InputAreaMsg::RemoveAttachment(index) => {
                if index < self.pending_images.len() {
                    let removed = self.pending_images.remove(index);
                    // Remove the widget from the FlowBox
                    if let Some(parent) = removed.container.parent() {
                        if let Ok(child) = parent.downcast::<gtk::FlowBoxChild>() {
                            self.attachment_strip.remove(&child);
                        }
                    }
                }
            }
            InputAreaMsg::PasteImage(png_data) => {
                self.add_image_from_bytes(png_data, "image/png", "clipboard.png", &sender);
            }
        }
    }
}

impl InputArea {
    fn get_text(&self) -> String {
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.text(&start, &end, false).to_string()
    }

    fn clear_attachment_strip(&self) {
        while let Some(child) = self.attachment_strip.first_child() {
            self.attachment_strip.remove(&child);
        }
    }

    fn add_image_from_path(&mut self, path: PathBuf, sender: &ComponentSender<Self>) {
        // Read file data
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("Failed to read image file: {}", e);
                return;
            }
        };

        // Determine MIME type from extension
        let mime_type = match path.extension().and_then(|e| e.to_str()) {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            _ => "image/png", // fallback
        }
        .to_string();

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image")
            .to_string();

        self.add_image_from_bytes(data, &mime_type, &filename, sender);
    }

    fn add_image_from_bytes(
        &mut self,
        data: Vec<u8>,
        mime_type: &str,
        filename: &str,
        sender: &ComponentSender<Self>,
    ) {
        // Create thumbnail
        let bytes = glib::Bytes::from(&data);
        let texture = match gtk::gdk::Texture::from_bytes(&bytes) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to create texture: {}", e);
                return;
            }
        };

        let image = gtk::Image::from_paintable(Some(&texture));
        image.set_pixel_size(80);
        image.add_css_class("attachment-thumbnail");

        // Build container with remove button
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&image));

        let remove_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .build();
        remove_btn.add_css_class("circular");
        remove_btn.add_css_class("osd");

        let index = self.pending_images.len();
        let sender_rm = sender.input_sender().clone();
        remove_btn.connect_clicked(move |_| {
            sender_rm.send(InputAreaMsg::RemoveAttachment(index)).unwrap();
        });
        overlay.add_overlay(&remove_btn);

        container.append(&overlay);

        let name_label = gtk::Label::builder()
            .label(filename)
            .max_width_chars(12)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        name_label.add_css_class("caption");
        container.append(&name_label);

        self.attachment_strip.append(&container);

        self.pending_images.push(PendingImage {
            mime_type: mime_type.to_string(),
            data,
            container,
        });
    }
}
