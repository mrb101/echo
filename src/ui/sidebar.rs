use adw::prelude::*;
use chrono::{Datelike, Utc};
use relm4::factory::FactoryVecDeque;
use relm4::prelude::*;

use crate::models::Conversation;

// --- SidebarItem: discriminated union for date headers vs conversation rows ---

#[derive(Debug, Clone)]
pub enum SidebarItem {
    Header(String),              // "Pinned", "Today", "Yesterday", etc.
    Conversation(Conversation),
}

// --- ConversationRow factory component ---

#[derive(Debug)]
pub struct ConversationRow {
    pub item: SidebarItem,
}

#[derive(Debug)]
pub enum ConversationRowMsg {}

#[derive(Debug)]
pub enum ConversationRowOutput {}

#[relm4::factory(pub)]
impl FactoryComponent for ConversationRow {
    type Init = SidebarItem;
    type Input = ConversationRowMsg;
    type Output = ConversationRowOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
            set_margin_all: 6,
        }
    }

    fn init_model(item: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { item }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
        match &self.item {
            SidebarItem::Header(label) => {
                let header_label = gtk::Label::builder()
                    .label(label)
                    .halign(gtk::Align::Start)
                    .margin_top(8)
                    .margin_bottom(2)
                    .margin_start(4)
                    .build();
                header_label.add_css_class("dim-label");
                header_label.add_css_class("caption");
                header_label.add_css_class("sidebar-date-header");
                root.append(&header_label);

                // Make header rows non-activatable/non-selectable
                returned_widget.set_activatable(false);
                returned_widget.set_selectable(false);
            }
            SidebarItem::Conversation(conv) => {
                // Title
                let title_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(4)
                    .build();

                if conv.pinned {
                    let pin_icon = gtk::Image::from_icon_name("view-pin-symbolic");
                    pin_icon.add_css_class("dim-label");
                    pin_icon.set_pixel_size(12);
                    title_box.append(&pin_icon);
                }

                let title_label = gtk::Label::builder()
                    .label(&conv.title)
                    .halign(gtk::Align::Start)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(30)
                    .build();
                title_label.add_css_class("heading");
                title_box.append(&title_label);

                root.append(&title_box);

                // Model
                let model_label = gtk::Label::builder()
                    .label(&conv.model)
                    .halign(gtk::Align::Start)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build();
                model_label.add_css_class("dim-label");
                model_label.add_css_class("caption");
                root.append(&model_label);

                // Preview excerpt
                if let Some(preview) = &conv.last_message_preview {
                    if !preview.is_empty() {
                        let preview_label = gtk::Label::builder()
                            .label(preview)
                            .halign(gtk::Align::Start)
                            .ellipsize(gtk::pango::EllipsizeMode::End)
                            .max_width_chars(35)
                            .build();
                        preview_label.add_css_class("dim-label");
                        preview_label.add_css_class("caption");
                        preview_label.set_opacity(0.7);
                        root.append(&preview_label);
                    }
                }
            }
        }

        let widgets = view_output!();
        widgets
    }
}

// --- Sidebar component ---

pub struct Sidebar {
    pub conversations: FactoryVecDeque<ConversationRow>,
    search_term: String,
}

#[derive(Debug)]
pub enum SidebarMsg {
    LoadConversations(Vec<Conversation>),
    NewChat,
    ConversationSelected(String),
    AddConversation(Conversation),
    RemoveConversation(String),
    UpdateConversationTitle(String, String),
    // Context menu
    ShowContextMenu(f64, f64, usize), // x, y, index
    RenameConversation(usize),
    DeleteConversation(usize),
    ExportConversation(usize),
    TogglePin(usize),
    // Rename dialog response
    DoRename(String, String), // id, new_title
    // Search
    SearchChanged(String),
}

#[derive(Debug)]
pub enum SidebarOutput {
    NewChat,
    ConversationSelected(String),
    DeleteConversation(String),
    RenameConversation(String, String), // id, new_title
    ExportConversation(String),         // id
    TogglePin(String, bool),            // id, new_pinned_state
}

#[relm4::component(pub)]
impl Component for Sidebar {
    type Init = ();
    type Input = SidebarMsg;
    type Output = SidebarOutput;
    type CommandOutput = ();

    view! {
        adw::ToolbarView {
            add_top_bar = &adw::HeaderBar {
                set_show_end_title_buttons: false,

                pack_start = &gtk::Button {
                    set_icon_name: "list-add-symbolic",
                    set_tooltip_text: Some("New Chat"),
                    connect_clicked => SidebarMsg::NewChat,
                },

                #[wrap(Some)]
                set_title_widget = &adw::WindowTitle {
                    set_title: "Conversations",
                },
            },

            #[wrap(Some)]
            set_content = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,

                #[name = "search_entry"]
                gtk::SearchEntry {
                    set_placeholder_text: Some("Search conversations..."),
                    set_margin_start: 8,
                    set_margin_end: 8,
                    set_margin_top: 4,
                    set_margin_bottom: 4,
                    connect_search_changed[sender] => move |entry| {
                        sender.input(SidebarMsg::SearchChanged(entry.text().to_string()));
                    },
                },

                gtk::ScrolledWindow {
                    set_hscrollbar_policy: gtk::PolicyType::Never,
                    set_vexpand: true,

                    #[local_ref]
                    conversation_list -> gtk::ListBox {
                        set_selection_mode: gtk::SelectionMode::Single,
                        add_css_class: "navigation-sidebar",
                    },
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let conversations = FactoryVecDeque::builder()
            .launch(gtk::ListBox::default())
            .detach();

        let model = Self {
            conversations,
            search_term: String::new(),
        };

        let conversation_list = model.conversations.widget();
        let widgets = view_output!();

        // Connect row-activated signal properly
        let conv_list = model.conversations.widget().clone();
        let sender_clone = sender.clone();
        conv_list.connect_row_activated(move |_, row| {
            let index = row.index() as usize;
            sender_clone.input(SidebarMsg::ConversationSelected(index.to_string()));
        });

        // Right-click context menu
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3); // right-click
        let conv_list2 = model.conversations.widget().clone();
        let sender_rc = sender.clone();
        gesture.connect_released(move |_, _, x, y| {
            // Find which row was clicked
            if let Some(row) = conv_list2.row_at_y(y as i32) {
                let index = row.index() as usize;
                sender_rc.input(SidebarMsg::ShowContextMenu(x, y, index));
            }
        });
        model.conversations.widget().add_controller(gesture);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            SidebarMsg::LoadConversations(conversations) => {
                let mut guard = self.conversations.guard();
                guard.clear();

                // Separate pinned and unpinned
                let (pinned, unpinned): (Vec<_>, Vec<_>) =
                    conversations.into_iter().partition(|c| c.pinned);

                // Add pinned group
                if !pinned.is_empty() {
                    guard.push_back(SidebarItem::Header("Pinned".to_string()));
                    for conv in pinned {
                        guard.push_back(SidebarItem::Conversation(conv));
                    }
                }

                // Group unpinned by date
                let mut current_group: Option<String> = None;
                for conv in unpinned {
                    let group = date_group(&conv.updated_at);
                    if current_group.as_deref() != Some(group) {
                        current_group = Some(group.to_string());
                        guard.push_back(SidebarItem::Header(group.to_string()));
                    }
                    guard.push_back(SidebarItem::Conversation(conv));
                }

                drop(guard);
                self.apply_search_filter();
            }
            SidebarMsg::NewChat => {
                let _ = sender.output(SidebarOutput::NewChat);
            }
            SidebarMsg::ConversationSelected(index_str) => {
                if let Ok(index) = index_str.parse::<usize>() {
                    let guard = self.conversations.guard();
                    if let Some(row) = guard.get(index) {
                        if let SidebarItem::Conversation(conv) = &row.item {
                            let id = conv.id.clone();
                            drop(guard);
                            let _ = sender.output(SidebarOutput::ConversationSelected(id));
                        }
                    }
                }
            }
            SidebarMsg::AddConversation(conversation) => {
                // Insert after the first header ("Today" or "Pinned")
                let mut guard = self.conversations.guard();

                // Find the right insertion point: after "Today" header, or create one
                let mut insert_at = None;
                let today_group = date_group(&conversation.updated_at);

                for i in 0..guard.len() {
                    if let Some(row) = guard.get(i) {
                        if let SidebarItem::Header(h) = &row.item {
                            if h == today_group {
                                insert_at = Some(i + 1);
                                break;
                            }
                        }
                    }
                }

                if let Some(idx) = insert_at {
                    guard.insert(idx, SidebarItem::Conversation(conversation));
                } else {
                    // Need to add the header too; find the first non-pinned header or end
                    let mut insert_header_at = 0;
                    for i in 0..guard.len() {
                        if let Some(row) = guard.get(i) {
                            match &row.item {
                                SidebarItem::Header(h) if h == "Pinned" => {
                                    // Skip pinned section
                                    continue;
                                }
                                SidebarItem::Conversation(c) if c.pinned => {
                                    continue;
                                }
                                _ => {
                                    insert_header_at = i;
                                    break;
                                }
                            }
                        }
                        insert_header_at = guard.len();
                    }
                    guard.insert(insert_header_at, SidebarItem::Header(today_group.to_string()));
                    guard.insert(insert_header_at + 1, SidebarItem::Conversation(conversation));
                }
                drop(guard);
                self.apply_search_filter();
            }
            SidebarMsg::RemoveConversation(id) => {
                let mut guard = self.conversations.guard();
                let pos = guard.iter().position(|r| {
                    matches!(&r.item, SidebarItem::Conversation(c) if c.id == id)
                });
                if let Some(index) = pos {
                    guard.remove(index);
                }
            }
            SidebarMsg::UpdateConversationTitle(id, title) => {
                let mut guard = self.conversations.guard();
                let pos = guard.iter().position(|r| {
                    matches!(&r.item, SidebarItem::Conversation(c) if c.id == id)
                });
                if let Some(index) = pos {
                    if let Some(row) = guard.get_mut(index) {
                        if let SidebarItem::Conversation(conv) = &mut row.item {
                            conv.title = title;
                        }
                    }
                }
            }
            SidebarMsg::ShowContextMenu(x, y, index) => {
                // Collect info from guard first, then drop it
                let (is_header, is_pinned) = {
                    let guard = self.conversations.guard();
                    let is_header = guard.get(index).map(|r| {
                        matches!(&r.item, SidebarItem::Header(_))
                    }).unwrap_or(true);
                    let is_pinned = guard.get(index).map(|r| {
                        matches!(&r.item, SidebarItem::Conversation(c) if c.pinned)
                    }).unwrap_or(false);
                    (is_header, is_pinned)
                };

                if is_header {
                    return;
                }

                let list_widget = self.conversations.widget();

                let menu = gio::Menu::new();
                if is_pinned {
                    menu.append(Some("Unpin"), Some("sidebar.toggle-pin"));
                } else {
                    menu.append(Some("Pin"), Some("sidebar.toggle-pin"));
                }
                menu.append(Some("Rename"), Some("sidebar.rename"));
                menu.append(Some("Export"), Some("sidebar.export"));
                menu.append(Some("Delete"), Some("sidebar.delete"));

                let action_group = gio::SimpleActionGroup::new();

                let sender_pin = sender.input_sender().clone();
                let pin_action = gio::SimpleAction::new("toggle-pin", None);
                let idx = index;
                pin_action.connect_activate(move |_, _| {
                    sender_pin.send(SidebarMsg::TogglePin(idx)).unwrap();
                });
                action_group.add_action(&pin_action);

                let sender_rename = sender.input_sender().clone();
                let rename_action = gio::SimpleAction::new("rename", None);
                let idx = index;
                rename_action.connect_activate(move |_, _| {
                    sender_rename.send(SidebarMsg::RenameConversation(idx)).unwrap();
                });
                action_group.add_action(&rename_action);

                let sender_export = sender.input_sender().clone();
                let export_action = gio::SimpleAction::new("export", None);
                let idx = index;
                export_action.connect_activate(move |_, _| {
                    sender_export.send(SidebarMsg::ExportConversation(idx)).unwrap();
                });
                action_group.add_action(&export_action);

                let sender_delete = sender.input_sender().clone();
                let delete_action = gio::SimpleAction::new("delete", None);
                let idx = index;
                delete_action.connect_activate(move |_, _| {
                    sender_delete.send(SidebarMsg::DeleteConversation(idx)).unwrap();
                });
                action_group.add_action(&delete_action);

                list_widget.insert_action_group("sidebar", Some(&action_group));

                let popover = gtk::PopoverMenu::from_model(Some(&menu));
                popover.set_parent(list_widget);
                popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                    x as i32,
                    y as i32,
                    1,
                    1,
                )));
                popover.set_has_arrow(true);

                // Clean up when popover is closed (delay to let action activate first)
                let parent = list_widget.clone();
                popover.connect_closed(move |p| {
                    let popover = p.clone();
                    let parent = parent.clone();
                    glib::idle_add_local_once(move || {
                        popover.unparent();
                        parent.insert_action_group("sidebar", None::<&gio::SimpleActionGroup>);
                    });
                });

                popover.popup();
            }
            SidebarMsg::RenameConversation(index) => {
                let guard = self.conversations.guard();
                let conv_data = guard.get(index).and_then(|r| {
                    if let SidebarItem::Conversation(c) = &r.item {
                        Some((c.id.clone(), c.title.clone()))
                    } else {
                        None
                    }
                });
                drop(guard);

                if let Some((id, current_title)) = conv_data {
                    // Show rename dialog using adw::AlertDialog
                    let dialog = adw::AlertDialog::builder()
                        .heading("Rename Conversation")
                        .body("Enter a new name:")
                        .build();

                    let entry = gtk::Entry::builder()
                        .text(&current_title)
                        .activates_default(true)
                        .build();

                    use adw::prelude::AlertDialogExt;
                    dialog.set_extra_child(Some(&entry));
                    dialog.add_response("cancel", "Cancel");
                    dialog.add_response("rename", "Rename");
                    dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
                    dialog.set_default_response(Some("rename"));
                    dialog.set_close_response("cancel");

                    let sender_dlg = sender.input_sender().clone();
                    let conv_id = id;
                    dialog.connect_response(None, move |_dialog, response| {
                        if response == "rename" {
                            let new_title = entry.text().to_string();
                            if !new_title.trim().is_empty() {
                                sender_dlg.send(SidebarMsg::DoRename(
                                    conv_id.clone(),
                                    new_title,
                                )).unwrap();
                            }
                        }
                    });

                    // Present dialog on the widget's window
                    use adw::prelude::AdwDialogExt;
                    if let Some(window) = root.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
                        dialog.present(Some(&window));
                    }
                }
            }
            SidebarMsg::DoRename(id, new_title) => {
                // Update locally
                let mut guard = self.conversations.guard();
                let pos = guard.iter().position(|r| {
                    matches!(&r.item, SidebarItem::Conversation(c) if c.id == id)
                });
                if let Some(index) = pos {
                    if let Some(row) = guard.get_mut(index) {
                        if let SidebarItem::Conversation(conv) = &mut row.item {
                            conv.title = new_title.clone();
                        }
                    }
                }
                drop(guard);

                let _ = sender.output(SidebarOutput::RenameConversation(id, new_title));
            }
            SidebarMsg::DeleteConversation(index) => {
                let guard = self.conversations.guard();
                let conv_id = guard.get(index).and_then(|r| {
                    if let SidebarItem::Conversation(c) = &r.item {
                        Some(c.id.clone())
                    } else {
                        None
                    }
                });
                drop(guard);

                if let Some(id) = conv_id {
                    let _ = sender.output(SidebarOutput::DeleteConversation(id));
                }
            }
            SidebarMsg::ExportConversation(index) => {
                let guard = self.conversations.guard();
                let conv_id = guard.get(index).and_then(|r| {
                    if let SidebarItem::Conversation(c) = &r.item {
                        Some(c.id.clone())
                    } else {
                        None
                    }
                });
                drop(guard);

                if let Some(id) = conv_id {
                    let _ = sender.output(SidebarOutput::ExportConversation(id));
                }
            }
            SidebarMsg::TogglePin(index) => {
                let guard = self.conversations.guard();
                let pin_data = guard.get(index).and_then(|r| {
                    if let SidebarItem::Conversation(c) = &r.item {
                        Some((c.id.clone(), !c.pinned))
                    } else {
                        None
                    }
                });
                drop(guard);

                if let Some((id, new_pinned)) = pin_data {
                    let _ = sender.output(SidebarOutput::TogglePin(id, new_pinned));
                }
            }
            SidebarMsg::SearchChanged(term) => {
                self.search_term = term.to_lowercase();
                self.apply_search_filter();
            }
        }
    }
}

impl Sidebar {
    fn apply_search_filter(&mut self) {
        let is_searching = !self.search_term.is_empty();

        // Collect filter data while holding guard
        let filter_data: Vec<(usize, bool, bool)> = {
            let guard = self.conversations.guard();
            (0..guard.len())
                .filter_map(|i| {
                    guard.get(i).map(|row_data| {
                        match &row_data.item {
                            SidebarItem::Header(_) => (i, true, true), // is_header = true
                            SidebarItem::Conversation(conv) => {
                                let visible = if is_searching {
                                    conv.title.to_lowercase().contains(&self.search_term)
                                } else {
                                    true
                                };
                                (i, visible, false) // is_header = false
                            }
                        }
                    })
                })
                .collect()
        };

        // When searching, hide headers; when not searching, show headers only if
        // they have visible conversations after them
        let list_widget = self.conversations.widget();

        if is_searching {
            for (i, visible, is_header) in &filter_data {
                if let Some(row) = list_widget.row_at_index(*i as i32) {
                    if *is_header {
                        row.set_visible(false);
                    } else {
                        row.set_visible(*visible);
                    }
                }
            }
        } else {
            // Show all when not searching
            for (i, visible, _) in &filter_data {
                if let Some(row) = list_widget.row_at_index(*i as i32) {
                    row.set_visible(*visible);
                }
            }
        }
    }
}

/// Classify a timestamp into a date group label.
fn date_group(dt: &chrono::DateTime<Utc>) -> &'static str {
    let now = Utc::now();
    let today = now.date_naive();
    let date = dt.date_naive();

    if date == today {
        "Today"
    } else if date == today.pred_opt().unwrap_or(today) {
        "Yesterday"
    } else if date.iso_week() == today.iso_week() && date.year() == today.year() {
        "This Week"
    } else {
        "Older"
    }
}
