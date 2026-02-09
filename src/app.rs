use std::sync::Arc;

use adw::prelude::*;
use chrono::Utc;
use relm4::prelude::*;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config;
use crate::models::{Account, Conversation, Message, ProviderId, Role};
use crate::providers::claude::ClaudeProvider;
use crate::providers::gemini::GeminiProvider;
use crate::providers::local::LocalProvider;
use crate::providers::ProviderRouter;
use crate::services::chat::{self, ChatDispatchParams, StreamResult};
use crate::services::settings::AppSettings;
use crate::services::{AccountService, Database, KeyringService, SettingsService};
use crate::ui::account_selector::{AccountSelector, AccountSelectorMsg, AccountSelectorOutput};
use crate::ui::chat_view::{ChatView, ChatViewMsg, ChatViewOutput};
use crate::ui::dialogs::account_setup::AccountSetupDialog;
use crate::ui::dialogs::system_prompt::{
    SystemPromptDialog, SystemPromptInit, SystemPromptOutput,
};
use crate::ui::preferences::accounts_page::{AccountsPage, AccountsPageMsg};
use crate::ui::preferences::appearance_page::{apply_color_scheme, AppearancePage};
use crate::ui::preferences::chat_page::ChatPage;
use crate::ui::onboarding::OnboardingWindow;
use crate::ui::sidebar::{Sidebar, SidebarMsg, SidebarOutput};

pub struct App {
    db: Database,
    account_service: Option<AccountService>,
    router: Arc<ProviderRouter>,
    sidebar: Controller<Sidebar>,
    chat_view: Controller<ChatView>,
    account_selector: Controller<AccountSelector>,
    active_conversation: Option<Conversation>,
    selected_account_id: Option<String>,
    selected_model: Option<String>,
    toast_overlay: adw::ToastOverlay,
    content_stack: gtk::Stack,
    initialized: bool,
    preferences_window: Option<adw::PreferencesWindow>,
    account_setup: Option<AsyncController<AccountSetupDialog>>,
    accounts_page: Option<Controller<AccountsPage>>,
    chat_page: Option<Controller<ChatPage>>,
    appearance_page: Option<Controller<AppearancePage>>,
    onboarding: Option<AsyncController<OnboardingWindow>>,
    system_prompt_dialog: Option<AsyncController<SystemPromptDialog>>,
    // Streaming state
    stream_cancel_token: Option<CancellationToken>,
    streaming_message_id: Option<String>,
    // Settings
    settings: AppSettings,
}

#[derive(Debug)]
pub enum AppMsg {
    NewChat,
    ConversationSelected(String),
    DeleteConversation(String),
    SendMessage(String, Vec<crate::providers::ImageAttachment>),
    AccountSelected(String),
    ModelSelected(String),
    InitComplete(Database, KeyringService),
    InitFailed(String),
    ShowToast(String),
    ShowPreferences,
    OpenAccountSetup,
    AccountAdded {
        provider: ProviderId,
        label: String,
        api_key: String,
        base_url: Option<String>,
        default_model: String,
        set_as_default: bool,
    },
    AccountSetupCancelled,
    DeleteAccountFromPrefs(String),
    ShowAbout,
    ShowOnboarding,
    OnboardingSetupProvider(ProviderId),
    OnboardingSkipped,
    StopGeneration,
    SettingsChanged(AppSettings),
    ShowSystemPromptDialog,
    SetConversationSystemPrompt(String, Option<String>),
    RenameConversation(String, String), // id, new_title
    ExportConversation(String),
    RegenerateMessage(String),        // message_id
    EditMessage(String, String),      // message_id, new_content
    TogglePin(String, bool),          // id, new_pinned_state
    ShowShortcuts,
    QuickSwitch,
}

#[derive(Debug)]
pub enum AppCmd {
    Initialized(Database, KeyringService),
    InitFailed(String),
    ConversationsLoaded(Vec<Conversation>),
    AccountsLoaded(Vec<Account>),
    MessagesLoaded(String, Vec<Message>),
    ChatResponse {
        conversation_id: String,
        content: String,
        model: String,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
        account_id: String,
    },
    ChatError(String),
    ConversationCreated(Conversation),
    AccountAddResult(Result<Account, String>),
    AccountDeleted(String),
    AccountsRefreshed(Vec<Account>),
    NeedsOnboarding(bool),
    // Streaming
    StreamToken {
        _conversation_id: String,
        message_id: String,
        token: String,
    },
    StreamDone {
        conversation_id: String,
        message_id: String,
        full_content: String,
        model: String,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
        account_id: String,
    },
    StreamError {
        _conversation_id: String,
        message_id: String,
        error: String,
    },
    SettingsLoaded(AppSettings),
    LocalModelsDiscovered {
        account_id: String,
        models: Vec<String>,
    },
}

#[relm4::component(pub, async)]
impl AsyncComponent for App {
    type Init = ();
    type Input = AppMsg;
    type Output = ();
    type CommandOutput = AppCmd;

    view! {
        adw::ApplicationWindow {
            set_title: Some(config::APP_NAME),
            set_default_width: 1200,
            set_default_height: 800,
            set_width_request: 620,
            set_height_request: 500,

            #[local_ref]
            toast_overlay -> adw::ToastOverlay {},
        }
    }

    async fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        // Set window size imperatively to ensure it takes effect
        root.set_default_size(1200, 800);

        let sidebar = Sidebar::builder()
            .launch(())
            .forward(sender.input_sender(), |output| match output {
                SidebarOutput::NewChat => AppMsg::NewChat,
                SidebarOutput::ConversationSelected(id) => AppMsg::ConversationSelected(id),
                SidebarOutput::DeleteConversation(id) => AppMsg::DeleteConversation(id),
                SidebarOutput::RenameConversation(id, title) => {
                    AppMsg::RenameConversation(id, title)
                }
                SidebarOutput::ExportConversation(id) => AppMsg::ExportConversation(id),
                SidebarOutput::TogglePin(id, pinned) => AppMsg::TogglePin(id, pinned),
            });

        let chat_view = ChatView::builder()
            .launch(())
            .forward(sender.input_sender(), |output| match output {
                ChatViewOutput::SendMessage { text, images } => {
                    AppMsg::SendMessage(text, images)
                }
                ChatViewOutput::StopGeneration => AppMsg::StopGeneration,
                ChatViewOutput::RegenerateMessage(msg_id) => AppMsg::RegenerateMessage(msg_id),
                ChatViewOutput::EditMessage(msg_id, content) => {
                    AppMsg::EditMessage(msg_id, content)
                }
            });

        let account_selector = AccountSelector::builder().launch(()).forward(
            sender.input_sender(),
            |output| match output {
                AccountSelectorOutput::AccountSelected(id) => AppMsg::AccountSelected(id),
                AccountSelectorOutput::ModelSelected(model) => AppMsg::ModelSelected(model),
            },
        );

        let mut router = ProviderRouter::new();
        router.register(Arc::new(GeminiProvider::new()));
        router.register(Arc::new(ClaudeProvider::new()));
        router.register(Arc::new(LocalProvider::new()));
        let router = Arc::new(router);

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_hexpand(true);
        toast_overlay.set_vexpand(true);

        // Build the content stack
        let content_stack = gtk::Stack::new();
        content_stack.set_hexpand(true);
        content_stack.set_vexpand(true);

        // Empty state page
        let empty_page = adw::StatusPage::new();
        empty_page.set_title("Start a New Conversation");
        empty_page.set_description(Some("Click \"New Chat\" to begin"));
        empty_page.set_icon_name(Some("chat-symbolic"));
        let new_chat_btn = gtk::Button::builder()
            .label("New Chat")
            .halign(gtk::Align::Center)
            .build();
        new_chat_btn.add_css_class("suggested-action");
        new_chat_btn.add_css_class("pill");
        let sender_btn = sender.input_sender().clone();
        new_chat_btn.connect_clicked(move |_| {
            sender_btn.send(AppMsg::NewChat).unwrap();
        });
        empty_page.set_child(Some(&new_chat_btn));
        content_stack.add_named(&empty_page, Some("empty"));

        // Chat page
        let chat_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        chat_box.set_hexpand(true);
        chat_box.set_vexpand(true);
        chat_box.append(chat_view.widget());
        content_stack.add_named(&chat_box, Some("chat"));

        content_stack.set_visible_child_name("empty");

        // Insert account selector into chat view's widget tree (between separator and input area)
        let cv_root = chat_view.widget();
        // The chat_view root is a gtk::Box with children:
        //   search_bar, overlay, loading_box, separator, input_area
        // We need to insert the selector row after the separator (4th child)
        let selector_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(12)
            .margin_end(12)
            .build();
        selector_row.add_css_class("selector-row");
        selector_row.append(account_selector.widget());

        // Find the separator (child before input_area widget)
        // input_area is the last child; separator is its previous sibling
        if let Some(input_widget) = cv_root.last_child() {
            if let Some(separator) = input_widget.prev_sibling() {
                cv_root.insert_child_after(&selector_row, Some(&separator));
            }
        }

        // Build toolbar view for content side
        let content_header = adw::HeaderBar::new();
        content_header.set_show_start_title_buttons(false);

        // System prompt button
        let system_prompt_btn = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("System Prompt")
            .build();
        let sender_sp = sender.input_sender().clone();
        system_prompt_btn.connect_clicked(move |_| {
            sender_sp.send(AppMsg::ShowSystemPromptDialog).unwrap();
        });
        content_header.pack_start(&system_prompt_btn);

        let content_toolbar = adw::ToolbarView::new();
        content_toolbar.add_top_bar(&content_header);
        content_toolbar.set_content(Some(&content_stack));

        let content_page = adw::NavigationPage::builder()
            .title("Chat")
            .tag("content")
            .child(&content_toolbar)
            .build();

        // Build sidebar with hamburger menu
        let sidebar_page = adw::NavigationPage::builder()
            .title("Conversations")
            .tag("sidebar")
            .child(sidebar.widget())
            .build();

        // Add hamburger menu to sidebar header
        let menu = gio::Menu::new();
        menu.append(Some("Preferences"), Some("app.preferences"));
        menu.append(Some("About Echo"), Some("app.about"));

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .build();

        content_header.pack_end(&menu_button);

        // Build split view
        let split_view = adw::NavigationSplitView::new();
        split_view.set_hexpand(true);
        split_view.set_vexpand(true);
        split_view.set_min_sidebar_width(200.0);
        split_view.set_max_sidebar_width(300.0);
        split_view.set_sidebar(Some(&sidebar_page));
        split_view.set_content(Some(&content_page));

        // Add breakpoint for adaptivity
        let breakpoint = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 600px")
                .expect("Invalid breakpoint condition"),
        );
        breakpoint.add_setter(&split_view, "collapsed", Some(&true.to_value()));
        breakpoint.add_setter(&content_header, "show-start-title-buttons", Some(&true.to_value()));
        root.add_breakpoint(breakpoint);

        toast_overlay.set_child(Some(&split_view));

        let model = App {
            db: Database::new_in_memory().expect("placeholder db"),
            account_service: None,
            router,
            sidebar,
            chat_view,
            account_selector,
            active_conversation: None,
            selected_account_id: None,
            selected_model: None,
            toast_overlay: toast_overlay.clone(),
            content_stack,
            initialized: false,
            preferences_window: None,
            account_setup: None,
            accounts_page: None,
            chat_page: None,
            appearance_page: None,
            onboarding: None,
            system_prompt_dialog: None,
            stream_cancel_token: None,
            streaming_message_id: None,
            settings: AppSettings::default(),
        };

        let widgets = view_output!();

        // Set up app actions
        let app = relm4::main_adw_application();
        let sender_prefs = sender.input_sender().clone();
        let prefs_action = gio::SimpleAction::new("preferences", None);
        prefs_action.connect_activate(move |_, _| {
            sender_prefs.send(AppMsg::ShowPreferences).unwrap();
        });
        app.add_action(&prefs_action);

        let sender_about = sender.input_sender().clone();
        let about_action = gio::SimpleAction::new("about", None);
        about_action.connect_activate(move |_, _| {
            sender_about.send(AppMsg::ShowAbout).unwrap();
        });
        app.add_action(&about_action);

        // Keyboard shortcuts
        let sender_new = sender.input_sender().clone();
        let new_chat_action = gio::SimpleAction::new("new-chat", None);
        new_chat_action.connect_activate(move |_, _| {
            sender_new.send(AppMsg::NewChat).unwrap();
        });
        app.add_action(&new_chat_action);
        app.set_accels_for_action("app.new-chat", &["<Control>n"]);
        app.set_accels_for_action("app.preferences", &["<Control>comma"]);

        let sender_stop = sender.input_sender().clone();
        let stop_action = gio::SimpleAction::new("stop-generation", None);
        stop_action.connect_activate(move |_, _| {
            sender_stop.send(AppMsg::StopGeneration).unwrap();
        });
        app.add_action(&stop_action);
        app.set_accels_for_action("app.stop-generation", &["Escape"]);

        // Ctrl+/ - Show keyboard shortcuts
        let sender_shortcuts = sender.input_sender().clone();
        let shortcuts_action = gio::SimpleAction::new("show-shortcuts", None);
        shortcuts_action.connect_activate(move |_, _| {
            sender_shortcuts.send(AppMsg::ShowShortcuts).unwrap();
        });
        app.add_action(&shortcuts_action);
        app.set_accels_for_action("app.show-shortcuts", &["<Control>slash"]);

        // Ctrl+F - Search in conversation
        let find_action = gio::SimpleAction::new("find-in-conversation", None);
        let chat_view_sender_find = model.chat_view.sender().clone();
        find_action.connect_activate(move |_, _| {
            chat_view_sender_find.send(ChatViewMsg::ToggleSearch).unwrap();
        });
        app.add_action(&find_action);
        app.set_accels_for_action("app.find-in-conversation", &["<Control>f"]);

        // Ctrl+K - Quick account/model switch
        let sender_quick = sender.input_sender().clone();
        let quick_action = gio::SimpleAction::new("quick-switch", None);
        quick_action.connect_activate(move |_, _| {
            sender_quick.send(AppMsg::QuickSwitch).unwrap();
        });
        app.add_action(&quick_action);
        app.set_accels_for_action("app.quick-switch", &["<Control>k"]);

        // Async initialization
        sender.command(|out, _| {
            Box::pin(async move {
                match Self::async_init().await {
                    Ok((db, keyring)) => out.send(AppCmd::Initialized(db, keyring)).unwrap(),
                    Err(e) => out.send(AppCmd::InitFailed(e.to_string())).unwrap(),
                }
            })
        });

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            AppMsg::NewChat => {
                if self.selected_account_id.is_none() {
                    self.show_toast("Please add an account first (use Preferences)");
                    return;
                }

                let account_id = self.selected_account_id.clone().unwrap();
                let model = self
                    .selected_model
                    .clone()
                    .unwrap_or_else(|| "gemini-2.5-flash".to_string());

                let now = Utc::now();
                let conversation = Conversation {
                    id: Uuid::new_v4().to_string(),
                    account_id,
                    title: "New Chat".to_string(),
                    model,
                    system_prompt: None,
                    pinned: false,
                    last_message_preview: None,
                    created_at: now,
                    updated_at: now,
                };

                let db = self.db.clone();
                let conv = conversation.clone();
                sender.command(move |out, _| {
                    Box::pin(async move {
                        if let Err(e) = db.insert_conversation(&conv).await {
                            out.send(AppCmd::ChatError(format!(
                                "Failed to create conversation: {}",
                                e
                            )))
                            .unwrap();
                        } else {
                            out.send(AppCmd::ConversationCreated(conv)).unwrap();
                        }
                    })
                });
            }
            AppMsg::ConversationSelected(id) => {
                let db = self.db.clone();
                let conv_id = id.clone();
                sender.command(move |out, _| {
                    Box::pin(async move {
                        match db.list_messages(&conv_id).await {
                            Ok(mut messages) => {
                                // Load attachments for user messages
                                for msg in &mut messages {
                                    if msg.role == Role::User {
                                        if let Ok(atts) = db.list_attachments(&msg.id).await {
                                            if !atts.is_empty() {
                                                msg.attachments = atts;
                                            }
                                        }
                                    }
                                }
                                out.send(AppCmd::MessagesLoaded(conv_id, messages)).unwrap()
                            }
                            Err(e) => out
                                .send(AppCmd::ChatError(format!(
                                    "Failed to load messages: {}",
                                    e
                                )))
                                .unwrap(),
                        }
                    })
                });
            }
            AppMsg::DeleteConversation(id) => {
                match self.db.delete_conversation(&id).await {
                    Ok(()) => {
                        self.sidebar
                            .emit(SidebarMsg::RemoveConversation(id.clone()));
                        if self
                            .active_conversation
                            .as_ref()
                            .is_some_and(|c| c.id == id)
                        {
                            self.active_conversation = None;
                            self.chat_view.emit(ChatViewMsg::Clear);
                            self.content_stack.set_visible_child_name("empty");
                        }
                    }
                    Err(e) => self.show_toast(&format!("Failed to delete: {}", e)),
                }
            }
            AppMsg::SendMessage(text, images) => {
                self.handle_send_message(text, images, sender).await;
            }
            AppMsg::AccountSelected(id) => {
                // If there's an active conversation bound to a different account, clear it
                // and auto-create a new chat with the new provider
                if let Some(conv) = &self.active_conversation {
                    if conv.account_id != id {
                        self.active_conversation = None;
                        self.chat_view.emit(ChatViewMsg::Clear);
                        self.selected_account_id = Some(id);
                        sender.input(AppMsg::NewChat);
                        return;
                    }
                }
                self.selected_account_id = Some(id);
            }
            AppMsg::ModelSelected(model) => {
                self.selected_model = Some(model.clone());
                // Update the active conversation's model in memory and DB
                if let Some(conv) = &mut self.active_conversation {
                    conv.model = model.clone();
                    let db = self.db.clone();
                    let conv_id = conv.id.clone();
                    sender.command(move |_out, _| {
                        Box::pin(async move {
                            if let Err(e) = db.update_conversation_model(&conv_id, &model).await {
                                tracing::error!("Failed to update conversation model: {}", e);
                            }
                        })
                    });
                }
            }
            AppMsg::InitComplete(db, keyring) => {

                self.db = db.clone();
                self.account_service = Some(AccountService::new(
                    db.clone(),
                    keyring,
                    self.router.clone(),
                ));
                self.initialized = true;

                // Load settings
                let db_settings = db.clone();
                sender.command(move |out, _| {
                    Box::pin(async move {
                        let settings = SettingsService::load(&db_settings).await;
                        out.send(AppCmd::SettingsLoaded(settings)).unwrap();
                    })
                });

                let db2 = db.clone();
                let db3 = db.clone();
                sender.command(move |out, _| {
                    Box::pin(async move {
                        // Check if onboarding is needed
                        match db3.has_any_accounts().await {
                            Ok(has) => out.send(AppCmd::NeedsOnboarding(!has)).unwrap(),
                            Err(e) => tracing::error!("Failed to check accounts: {}", e),
                        }
                        match db.list_conversations().await {
                            Ok(convos) => out.send(AppCmd::ConversationsLoaded(convos)).unwrap(),
                            Err(e) => tracing::error!("Failed to load conversations: {}", e),
                        }
                        match db2.list_accounts().await {
                            Ok(accounts) => out.send(AppCmd::AccountsLoaded(accounts)).unwrap(),
                            Err(e) => tracing::error!("Failed to load accounts: {}", e),
                        }
                    })
                });

            }
            AppMsg::InitFailed(err) => {
                tracing::error!("Initialization failed: {}", err);
                self.show_toast(&format!("Error: {}", err));
            }
            AppMsg::ShowToast(msg) => {
                self.show_toast(&msg);
            }
            AppMsg::ShowPreferences => {
                self.show_preferences(root, sender.input_sender().clone());
            }
            AppMsg::OpenAccountSetup => {
                self.open_account_setup(root, sender.input_sender().clone(), ProviderId::Gemini);
            }
            AppMsg::AccountAdded {
                provider,
                label,
                api_key,
                base_url,
                default_model,
                set_as_default,
            } => {
                // Close the setup dialog
                self.account_setup = None;

                if let Some(service) = &self.account_service {
                    let service_db = self.db.clone();
                    let service_keyring = service.keyring_clone();
                    let router = self.router.clone();

                    let account_service =
                        AccountService::new(service_db, service_keyring, router);

                    sender.command(move |out, _| {
                        Box::pin(async move {
                            match account_service
                                .add_account(
                                    provider,
                                    label,
                                    api_key,
                                    base_url,
                                    default_model,
                                    set_as_default,
                                )
                                .await
                            {
                                Ok(account) => {
                                    out.send(AppCmd::AccountAddResult(Ok(account))).unwrap()
                                }
                                Err(e) => out
                                    .send(AppCmd::AccountAddResult(Err(e.to_string())))
                                    .unwrap(),
                            }
                        })
                    });
                } else {
                    self.show_toast("App not fully initialized");
                }
            }
            AppMsg::AccountSetupCancelled => {
                self.account_setup = None;
            }
            AppMsg::DeleteAccountFromPrefs(id) => {
                if let Some(service) = &self.account_service {
                    let db = self.db.clone();
                    let keyring = service.keyring_clone();
                    let router = self.router.clone();
                    let account_service = AccountService::new(db, keyring, router);
                    let aid = id.clone();

                    sender.command(move |out, _| {
                        Box::pin(async move {
                            match account_service.delete_account(&aid).await {
                                Ok(()) => out.send(AppCmd::AccountDeleted(aid)).unwrap(),
                                Err(e) => {
                                    out.send(AppCmd::ChatError(format!(
                                        "Failed to delete account: {}",
                                        e
                                    )))
                                    .unwrap()
                                }
                            }
                        })
                    });
                }
            }
            AppMsg::ShowAbout => {
                crate::ui::window::create_about_dialog(root);
            }
            AppMsg::ShowOnboarding => {

                self.show_onboarding(root, sender.input_sender().clone());

            }
            AppMsg::OnboardingSetupProvider(provider) => {
                self.onboarding = None;
                self.open_account_setup(root, sender.input_sender().clone(), provider);
            }
            AppMsg::OnboardingSkipped => {
                self.onboarding = None;
            }
            AppMsg::StopGeneration => {
                if let Some(token) = self.stream_cancel_token.take() {
                    token.cancel();
                }
                self.chat_view.emit(ChatViewMsg::SetLoading(false));

                // If there's a streaming message, complete it with partial content
                if let Some(msg_id) = self.streaming_message_id.take() {
                    self.chat_view
                        .emit(ChatViewMsg::StreamingComplete(msg_id));
                }
            }
            AppMsg::ShowSystemPromptDialog => {
                if let Some(conv) = &self.active_conversation {
                    let dialog = SystemPromptDialog::builder()
                        .launch(SystemPromptInit {
                            conversation_id: conv.id.clone(),
                            current_prompt: conv.system_prompt.clone(),
                        })
                        .forward(sender.input_sender(), |output| match output {
                            SystemPromptOutput::Updated(id, prompt) => {
                                AppMsg::SetConversationSystemPrompt(id, prompt)
                            }
                            SystemPromptOutput::Cancelled => {
                                AppMsg::ShowToast("".to_string()) // no-op
                            }
                        });

                    dialog.widget().set_transient_for(Some(root));
                    dialog.widget().present();
                    self.system_prompt_dialog = Some(dialog);
                } else {
                    self.show_toast("No active conversation");
                }
            }
            AppMsg::SetConversationSystemPrompt(conv_id, prompt) => {
                self.system_prompt_dialog = None;

                // Update in DB
                let db = self.db.clone();
                let prompt_for_db = prompt.clone();
                let cid = conv_id.clone();
                sender.command(move |_out, _| {
                    Box::pin(async move {
                        if let Err(e) = db
                            .update_conversation_system_prompt(
                                &cid,
                                prompt_for_db.as_deref(),
                            )
                            .await
                        {
                            tracing::error!("Failed to update system prompt: {}", e);
                        }
                    })
                });

                // Update local state
                if let Some(conv) = &mut self.active_conversation {
                    if conv.id == conv_id {
                        conv.system_prompt = prompt;
                    }
                }
            }
            AppMsg::RenameConversation(id, new_title) => {
                let db = self.db.clone();
                let cid = id.clone();
                let title = new_title.clone();
                sender.command(move |_out, _| {
                    Box::pin(async move {
                        if let Err(e) = db.update_conversation_title(&cid, &title).await {
                            tracing::error!("Failed to rename conversation: {}", e);
                        }
                    })
                });
                // Update local state
                if let Some(conv) = &mut self.active_conversation {
                    if conv.id == id {
                        conv.title = new_title;
                    }
                }
            }
            AppMsg::ExportConversation(id) => {
                self.handle_export_conversation(id, root, sender).await;
            }
            AppMsg::RegenerateMessage(msg_id) => {
                self.handle_regenerate(msg_id, sender).await;
            }
            AppMsg::EditMessage(msg_id, new_content) => {
                self.handle_edit_message(msg_id, new_content, sender).await;
            }
            AppMsg::SettingsChanged(settings) => {
                self.settings = settings.clone();
                // Apply color scheme immediately
                apply_color_scheme(settings.color_scheme);
                // Persist settings
                let db = self.db.clone();
                sender.command(move |_out, _| {
                    Box::pin(async move {
                        if let Err(e) = SettingsService::save(&db, &settings).await {
                            tracing::error!("Failed to save settings: {}", e);
                        }
                    })
                });
            }
            AppMsg::TogglePin(id, pinned) => {
                let db = self.db.clone();
                let cid = id.clone();
                sender.command(move |out, _| {
                    Box::pin(async move {
                        if let Err(e) = db.toggle_conversation_pin(&cid, pinned).await {
                            tracing::error!("Failed to toggle pin: {}", e);
                        }
                        // Reload conversations to reflect new order
                        match db.list_conversations().await {
                            Ok(convos) => out.send(AppCmd::ConversationsLoaded(convos)).unwrap(),
                            Err(e) => tracing::error!("Failed to reload conversations: {}", e),
                        }
                    })
                });
            }
            AppMsg::ShowShortcuts => {
                crate::ui::window::create_shortcuts_window(root);
            }
            AppMsg::QuickSwitch => {
                self.account_selector.emit(AccountSelectorMsg::GrabFocus);
            }
        }
    }

    async fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {

        match msg {
            AppCmd::Initialized(db, keyring) => {

                sender.input(AppMsg::InitComplete(db, keyring));
            }
            AppCmd::InitFailed(err) => {

                sender.input(AppMsg::InitFailed(err));
            }
            AppCmd::ConversationsLoaded(conversations) => {

                self.sidebar
                    .emit(SidebarMsg::LoadConversations(conversations));
            }
            AppCmd::AccountsLoaded(accounts) => {

                if !accounts.is_empty() {
                    let default = accounts
                        .iter()
                        .find(|a| a.is_default)
                        .or(accounts.first());
                    if let Some(acc) = default {
                        self.selected_account_id = Some(acc.id.clone());
                        self.selected_model = Some(acc.default_model.clone());
                    }
                    self.discover_local_models(&accounts, &sender);
                    self.account_selector
                        .emit(AccountSelectorMsg::SetAccounts(accounts));
                }
            }
            AppCmd::MessagesLoaded(conv_id, messages) => {
                // Load the full conversation from DB to get system_prompt etc.
                match self.db.get_conversation(&conv_id).await {
                    Ok(Some(conv)) => {
                        // Sync dropdowns to this conversation's account/model
                        self.account_selector.emit(AccountSelectorMsg::SyncToConversation(
                            conv.account_id.clone(),
                            conv.model.clone(),
                        ));
                        self.selected_account_id = Some(conv.account_id.clone());
                        self.selected_model = Some(conv.model.clone());
                        self.active_conversation = Some(conv);
                    }
                    _ => {
                        self.active_conversation = Some(Conversation {
                            id: conv_id.clone(),
                            account_id: self.selected_account_id.clone().unwrap_or_default(),
                            title: "Chat".to_string(),
                            model: self
                                .selected_model
                                .clone()
                                .unwrap_or_else(|| "gemini-2.5-flash".to_string()),
                            system_prompt: None,
                            pinned: false,
                            last_message_preview: None,
                            created_at: Utc::now(),
                            updated_at: Utc::now(),
                        });
                    }
                }
                self.chat_view.emit(ChatViewMsg::LoadMessages(messages));
                self.content_stack.set_visible_child_name("chat");
            }
            AppCmd::ChatResponse {
                conversation_id,
                content,
                model,
                tokens_in,
                tokens_out,
                account_id,
            } => {
                let now = Utc::now();
                let assistant_msg = Message {
                    id: Uuid::new_v4().to_string(),
                    conversation_id: conversation_id.clone(),
                    role: Role::Assistant,
                    content,
                    model: Some(model),
                    tokens_in,
                    tokens_out,
                    parent_message_id: None,
                    is_active: true,
                    created_at: now,
                    attachments: Vec::new(),
                };

                if let Err(e) = self.db.insert_message(&assistant_msg).await {
                    tracing::error!("Failed to save assistant message: {}", e);
                }

                let _ = self
                    .db
                    .update_conversation_timestamp(&conversation_id)
                    .await;

                if let (Some(ti), Some(to)) = (tokens_in, tokens_out) {
                    let _ = self.db.update_account_usage(&account_id, ti, to).await;
                }

                self.chat_view.emit(ChatViewMsg::AddMessage(assistant_msg));
                self.chat_view.emit(ChatViewMsg::SetLoading(false));
            }
            AppCmd::ChatError(err) => {
                self.show_toast(&err);
                self.chat_view.emit(ChatViewMsg::SetLoading(false));
            }
            AppCmd::ConversationCreated(conv) => {
                self.sidebar
                    .emit(SidebarMsg::AddConversation(conv.clone()));
                // Sync dropdowns to the new conversation's account/model
                self.account_selector.emit(AccountSelectorMsg::SyncToConversation(
                    conv.account_id.clone(),
                    conv.model.clone(),
                ));
                self.selected_account_id = Some(conv.account_id.clone());
                self.selected_model = Some(conv.model.clone());
                self.active_conversation = Some(conv);
                self.chat_view.emit(ChatViewMsg::Clear);
                self.content_stack.set_visible_child_name("chat");
            }
            AppCmd::AccountAddResult(result) => {

                match result {
                    Ok(account) => {
                        self.show_toast(&format!("Account '{}' added successfully", account.label));
                        // Refresh accounts
                        self.refresh_accounts(sender).await;
                    }
                    Err(e) => {
                        self.show_toast(&format!("Failed to add account: {}", e));
                    }
                }
            }
            AppCmd::AccountDeleted(id) => {
                self.show_toast("Account deleted");
                // If this was the selected account, clear it
                if self.selected_account_id.as_deref() == Some(&id) {
                    self.selected_account_id = None;
                    self.selected_model = None;
                }
                self.refresh_accounts(sender).await;
            }
            AppCmd::NeedsOnboarding(needs) => {

                if needs {
                    sender.input(AppMsg::ShowOnboarding);
                }
            }
            AppCmd::AccountsRefreshed(accounts) => {

                if let Some(page) = &self.accounts_page {
                    page.emit(AccountsPageMsg::SetAccounts(accounts.clone()));
                }
                if !accounts.is_empty() {
                    let default = accounts
                        .iter()
                        .find(|a| a.is_default)
                        .or(accounts.first());
                    if let Some(acc) = default {
                        if self.selected_account_id.is_none() {
                            self.selected_account_id = Some(acc.id.clone());
                            self.selected_model = Some(acc.default_model.clone());
                        }
                    }
                    self.discover_local_models(&accounts, &sender);
                    self.account_selector
                        .emit(AccountSelectorMsg::SetAccounts(accounts));
                }
            }
            // Streaming commands
            AppCmd::StreamToken {
                _conversation_id: _,
                message_id,
                token,
            } => {
                self.chat_view
                    .emit(ChatViewMsg::UpdateStreamingMessage(message_id, token));
            }
            AppCmd::StreamDone {
                conversation_id,
                message_id,
                full_content,
                model,
                tokens_in,
                tokens_out,
                account_id,
            } => {
                self.stream_cancel_token = None;
                self.streaming_message_id = None;

                // Save the complete message to DB
                let now = Utc::now();
                let assistant_msg = Message {
                    id: message_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: Role::Assistant,
                    content: full_content,
                    model: Some(model),
                    tokens_in,
                    tokens_out,
                    parent_message_id: None,
                    is_active: true,
                    created_at: now,
                    attachments: Vec::new(),
                };

                if let Err(e) = self.db.insert_message(&assistant_msg).await {
                    tracing::error!("Failed to save assistant message: {}", e);
                }

                let _ = self
                    .db
                    .update_conversation_timestamp(&conversation_id)
                    .await;

                if let (Some(ti), Some(to)) = (tokens_in, tokens_out) {
                    let _ = self.db.update_account_usage(&account_id, ti, to).await;
                }

                self.chat_view
                    .emit(ChatViewMsg::StreamingComplete(message_id.clone()));
                self.chat_view.emit(ChatViewMsg::SetMessageTokens(
                    message_id,
                    tokens_in,
                    tokens_out,
                ));
                self.chat_view.emit(ChatViewMsg::SetLoading(false));
            }
            AppCmd::StreamError {
                _conversation_id: _,
                message_id,
                error,
            } => {
                self.stream_cancel_token = None;
                self.streaming_message_id = None;

                self.show_toast(&format!("AI error: {}", error));
                self.chat_view.emit(ChatViewMsg::RemoveMessage(message_id));
                self.chat_view.emit(ChatViewMsg::SetLoading(false));
            }
            AppCmd::SettingsLoaded(settings) => {

                self.settings = settings;
                apply_color_scheme(self.settings.color_scheme);

            }
            AppCmd::LocalModelsDiscovered { account_id, models } => {
                self.account_selector
                    .emit(AccountSelectorMsg::SetLocalModels(account_id, models));
            }
        }
    }
}

impl App {
    async fn async_init() -> anyhow::Result<(Database, KeyringService)> {
        let db = Database::new().await?;
        let keyring = KeyringService::new().await?;
        Ok((db, keyring))
    }

    fn show_toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        toast.set_timeout(3);
        self.toast_overlay.add_toast(toast);
    }

    fn show_preferences(
        &mut self,
        parent: &adw::ApplicationWindow,
        sender: relm4::Sender<AppMsg>,
    ) {
        let handles =
            crate::ui::window::create_preferences_window(parent, &sender, &self.db, &self.settings);
        self.preferences_window = Some(handles.window);
        self.accounts_page = Some(handles.accounts_page);
        self.chat_page = Some(handles.chat_page);
        self.appearance_page = Some(handles.appearance_page);
    }

    fn open_account_setup(
        &mut self,
        parent: &adw::ApplicationWindow,
        sender: relm4::Sender<AppMsg>,
        provider: ProviderId,
    ) {
        self.account_setup =
            Some(crate::ui::window::create_account_setup(parent, &sender, provider));
    }

    fn show_onboarding(
        &mut self,
        parent: &adw::ApplicationWindow,
        sender: relm4::Sender<AppMsg>,
    ) {
        self.onboarding = Some(crate::ui::window::create_onboarding(parent, &sender));
    }

    fn discover_local_models(
        &self,
        accounts: &[Account],
        sender: &AsyncComponentSender<Self>,
    ) {
        for account in accounts {
            if account.provider != ProviderId::Local {
                continue;
            }
            let base_url = match &account.api_base_url {
                Some(url) => url.clone(),
                None => continue,
            };
            let account_id = account.id.clone();
            let router = self.router.clone();
            let keyring = match &self.account_service {
                Some(s) => s.keyring_clone(),
                None => continue,
            };
            sender.command(move |out, _| {
                Box::pin(async move {
                    // Get the API key for this account
                    let key_ref = format!("local:{}", &account_id);
                    let api_key = match keyring.retrieve(&key_ref).await {
                        Ok(Some(key)) => key,
                        _ => String::new(),
                    };
                    match router
                        .validate_credentials(
                            &ProviderId::Local,
                            &api_key,
                            Some(&base_url),
                        )
                        .await
                    {
                        Ok(model_infos) => {
                            let models: Vec<String> =
                                model_infos.into_iter().map(|m| m.id).collect();
                            let _ = out.send(AppCmd::LocalModelsDiscovered {
                                account_id,
                                models,
                            });
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to discover models for local account {}: {}",
                                account_id,
                                e
                            );
                        }
                    }
                })
            });
        }
    }

    async fn refresh_accounts(&self, sender: AsyncComponentSender<Self>) {
        let db = self.db.clone();
        sender.command(move |out, _| {
            Box::pin(async move {
                match db.list_accounts().await {
                    Ok(accounts) => out.send(AppCmd::AccountsRefreshed(accounts)).unwrap(),
                    Err(e) => tracing::error!("Failed to refresh accounts: {}", e),
                }
            })
        });
    }

    async fn handle_export_conversation(
        &mut self,
        conv_id: String,
        root: &adw::ApplicationWindow,
        _sender: AsyncComponentSender<Self>,
    ) {
        let conv = match self.db.get_conversation(&conv_id).await {
            Ok(Some(c)) => c,
            _ => {
                self.show_toast("Failed to load conversation");
                return;
            }
        };
        let messages = match self.db.list_messages(&conv_id).await {
            Ok(m) => m,
            Err(e) => {
                self.show_toast(&format!("Failed to load messages: {}", e));
                return;
            }
        };

        let markdown = crate::services::export::export_to_markdown(&conv, &messages);
        let filename = format!("{}.md", conv.title.replace(['/', '\\'], "_"));

        let dialog = gtk::FileDialog::builder()
            .title("Export Conversation")
            .initial_name(&filename)
            .build();

        let toast_overlay = self.toast_overlay.clone();
        dialog.save(Some(root), None::<&gio::Cancellable>, move |result| {
            match result {
                Ok(file) => {
                    if let Some(path) = file.path() {
                        match std::fs::write(&path, &markdown) {
                            Ok(()) => {
                                let toast = adw::Toast::new("Conversation exported");
                                toast.set_timeout(3);
                                toast_overlay.add_toast(toast);
                            }
                            Err(e) => {
                                let toast =
                                    adw::Toast::new(&format!("Export failed: {}", e));
                                toast.set_timeout(3);
                                toast_overlay.add_toast(toast);
                            }
                        }
                    }
                }
                Err(_) => {} // User cancelled
            }
        });
    }

    async fn handle_send_message(
        &mut self,
        text: String,
        images: Vec<crate::providers::ImageAttachment>,
        sender: AsyncComponentSender<Self>,
    ) {
        if self.active_conversation.is_none() {
            if self.selected_account_id.is_none() {
                self.show_toast("Please add an account first");
                return;
            }

            let account_id = self.selected_account_id.clone().unwrap();
            let model = self
                .selected_model
                .clone()
                .unwrap_or_else(|| "gemini-2.5-flash".to_string());

            let now = Utc::now();
            let title = truncate_title(&text);

            let conversation = Conversation {
                id: Uuid::new_v4().to_string(),
                account_id,
                title,
                model,
                system_prompt: None,
                pinned: false,
                last_message_preview: None,
                created_at: now,
                updated_at: now,
            };

            if let Err(e) = self.db.insert_conversation(&conversation).await {
                self.show_toast(&format!("Failed to create conversation: {}", e));
                return;
            }

            self.sidebar
                .emit(SidebarMsg::AddConversation(conversation.clone()));
            self.active_conversation = Some(conversation);
            self.content_stack.set_visible_child_name("chat");
        }

        let conv = self.active_conversation.as_ref().unwrap();
        let conversation_id = conv.id.clone();

        let now = Utc::now();
        let user_msg_id = Uuid::new_v4().to_string();

        // Build attachments for display in the message bubble
        let msg_attachments: Vec<crate::models::Attachment> = images
            .iter()
            .map(|img| crate::models::Attachment {
                id: String::new(),
                message_id: user_msg_id.clone(),
                mime_type: img.mime_type.clone(),
                filename: None,
                data: img.data.clone(),
                created_at: now,
            })
            .collect();

        let user_msg = Message {
            id: user_msg_id.clone(),
            conversation_id: conversation_id.clone(),
            role: Role::User,
            content: text.clone(),
            model: None,
            tokens_in: None,
            tokens_out: None,
            parent_message_id: None,
            is_active: true,
            created_at: now,
            attachments: msg_attachments,
        };

        if let Err(e) = self.db.insert_message(&user_msg).await {
            self.show_toast(&format!("Failed to save message: {}", e));
            return;
        }

        self.chat_view.emit(ChatViewMsg::AddMessage(user_msg));
        self.chat_view.emit(ChatViewMsg::SetLoading(true));

        let is_first_message = match self.db.list_messages(&conversation_id).await {
            Ok(msgs) => msgs.len() == 1,
            Err(_) => false,
        };

        if is_first_message {
            let title = truncate_title(&text);
            let _ = self
                .db
                .update_conversation_title(&conversation_id, &title)
                .await;
            self.sidebar.emit(SidebarMsg::UpdateConversationTitle(
                conversation_id.clone(),
                title,
            ));
        }

        let all_messages = match self.db.list_messages(&conversation_id).await {
            Ok(msgs) => msgs,
            Err(e) => {
                self.show_toast(&format!("Failed to load messages: {}", e));
                self.chat_view.emit(ChatViewMsg::SetLoading(false));
                return;
            }
        };

        let mut chat_messages = chat::messages_to_chat_messages(&all_messages);

        // Attach images to the last (current user) message
        if !images.is_empty() {
            if let Some(last_msg) = chat_messages.last_mut() {
                last_msg.images = images.clone();
            }

            // Save attachments to DB
            for img in &images {
                let attachment = crate::models::Attachment {
                    id: Uuid::new_v4().to_string(),
                    message_id: user_msg_id.clone(),
                    mime_type: img.mime_type.clone(),
                    filename: None,
                    data: img.data.clone(),
                    created_at: Utc::now(),
                };
                if let Err(e) = self.db.insert_attachment(&attachment).await {
                    tracing::error!("Failed to save attachment: {}", e);
                }
            }
        }

        let account_service = match &self.account_service {
            Some(s) => s,
            None => {
                self.show_toast("App not fully initialized");
                self.chat_view.emit(ChatViewMsg::SetLoading(false));
                return;
            }
        };

        let (account, api_key) =
            match account_service.get_account_with_key(&conv.account_id).await {
                Ok(pair) => pair,
                Err(e) => {
                    self.show_toast(&format!("Failed to get API key: {}", e));
                    self.chat_view.emit(ChatViewMsg::SetLoading(false));
                    return;
                }
            };

        let system_prompt = conv
            .system_prompt
            .clone()
            .or_else(|| self.settings.default_system_prompt.clone())
            .filter(|s| !s.trim().is_empty());

        let request = chat::build_request(
            api_key,
            &conv.model,
            chat_messages,
            &account,
            &self.settings,
            system_prompt,
        );

        let params = ChatDispatchParams {
            request,
            provider: account.provider,
            conversation_id: conversation_id.clone(),
            account_id: conv.account_id.clone(),
            model_name: conv.model.clone(),
        };

        self.dispatch_ai_request(params, sender);
    }

    async fn handle_regenerate(
        &mut self,
        assistant_msg_id: String,
        sender: AsyncComponentSender<Self>,
    ) {
        let conv = match &self.active_conversation {
            Some(c) => c,
            None => {
                self.show_toast("No active conversation");
                return;
            }
        };

        let remaining_messages = match crate::services::conversation::prepare_regeneration(
            &self.db,
            &conv.id,
            &assistant_msg_id,
        )
        .await
        {
            Ok(msgs) => msgs,
            Err(e) => {
                self.show_toast(&format!("{}", e));
                return;
            }
        };

        self.chat_view
            .emit(ChatViewMsg::LoadMessages(remaining_messages.clone()));
        self.send_to_ai(remaining_messages, sender).await;
    }

    async fn handle_edit_message(
        &mut self,
        msg_id: String,
        new_content: String,
        sender: AsyncComponentSender<Self>,
    ) {
        let conv = match &self.active_conversation {
            Some(c) => c,
            None => {
                self.show_toast("No active conversation");
                return;
            }
        };

        let active_messages = match crate::services::conversation::prepare_edit(
            &self.db,
            &conv.id,
            &msg_id,
            &new_content,
        )
        .await
        {
            Ok(msgs) => msgs,
            Err(e) => {
                self.show_toast(&format!("{}", e));
                return;
            }
        };

        self.chat_view
            .emit(ChatViewMsg::LoadMessages(active_messages.clone()));
        self.send_to_ai(active_messages, sender).await;
    }

    async fn send_to_ai(
        &mut self,
        messages: Vec<Message>,
        sender: AsyncComponentSender<Self>,
    ) {
        let conv = match &self.active_conversation {
            Some(c) => c,
            None => return,
        };

        let account_service = match &self.account_service {
            Some(s) => s,
            None => {
                self.show_toast("App not fully initialized");
                return;
            }
        };

        let (account, api_key) =
            match account_service.get_account_with_key(&conv.account_id).await {
                Ok(pair) => pair,
                Err(e) => {
                    self.show_toast(&format!("Failed to get API key: {}", e));
                    return;
                }
            };

        let system_prompt = conv
            .system_prompt
            .clone()
            .or_else(|| self.settings.default_system_prompt.clone())
            .filter(|s| !s.trim().is_empty());

        let chat_messages = chat::messages_to_chat_messages(&messages);
        let request =
            chat::build_request(api_key, &conv.model, chat_messages, &account, &self.settings, system_prompt);

        let params = ChatDispatchParams {
            request,
            provider: account.provider,
            conversation_id: conv.id.clone(),
            account_id: conv.account_id.clone(),
            model_name: conv.model.clone(),
        };

        self.chat_view.emit(ChatViewMsg::SetLoading(true));
        self.dispatch_ai_request(params, sender);
    }

    fn dispatch_ai_request(
        &mut self,
        params: ChatDispatchParams,
        sender: AsyncComponentSender<Self>,
    ) {
        let router = self.router.clone();

        if self.settings.stream_responses {
            let message_id = chat::new_message_id();
            self.streaming_message_id = Some(message_id.clone());

            let placeholder = Message {
                id: message_id.clone(),
                conversation_id: params.conversation_id.clone(),
                role: Role::Assistant,
                content: String::new(),
                model: Some(params.model_name.clone()),
                tokens_in: None,
                tokens_out: None,
                parent_message_id: None,
                is_active: true,
                created_at: Utc::now(),
                attachments: Vec::new(),
            };
            self.chat_view
                .emit(ChatViewMsg::AddStreamingMessage(placeholder));

            let cancel_token = CancellationToken::new();
            self.stream_cancel_token = Some(cancel_token.clone());

            sender.command(move |out, _| {
                Box::pin(async move {
                    chat::run_streaming(router, params, cancel_token, message_id, |event| {
                        match event {
                            StreamResult::Token {
                                conversation_id,
                                message_id,
                                accumulated,
                            } => {
                                out.send(AppCmd::StreamToken {
                                    _conversation_id: conversation_id,
                                    message_id,
                                    token: accumulated,
                                })
                                .unwrap();
                            }
                            StreamResult::Done {
                                conversation_id,
                                message_id,
                                full_content,
                                model,
                                tokens_in,
                                tokens_out,
                                account_id,
                            } => {
                                out.send(AppCmd::StreamDone {
                                    conversation_id,
                                    message_id,
                                    full_content,
                                    model,
                                    tokens_in,
                                    tokens_out,
                                    account_id,
                                })
                                .unwrap();
                            }
                            StreamResult::Error {
                                conversation_id,
                                message_id,
                                error,
                            } => {
                                out.send(AppCmd::StreamError {
                                    _conversation_id: conversation_id,
                                    message_id,
                                    error,
                                })
                                .unwrap();
                            }
                        }
                    })
                    .await;
                })
            });
        } else {
            sender.command(move |out, _| {
                Box::pin(async move {
                    match chat::send_non_streaming(router, params).await {
                        Ok(result) => {
                            out.send(AppCmd::ChatResponse {
                                conversation_id: result.conversation_id,
                                content: result.content,
                                model: result.model,
                                tokens_in: result.tokens_in,
                                tokens_out: result.tokens_out,
                                account_id: result.account_id,
                            })
                            .unwrap();
                        }
                        Err(e) => {
                            out.send(AppCmd::ChatError(e)).unwrap();
                        }
                    }
                })
            });
        }
    }
}

use crate::services::conversation::truncate_title;
