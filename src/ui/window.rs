use adw::prelude::*;
use relm4::prelude::*;

use crate::app::AppMsg;
use crate::config;
use crate::models::ProviderId;
use crate::services::database::Database;
use crate::services::settings::AppSettings;
use crate::ui::dialogs::account_setup::{AccountSetupDialog, AccountSetupOutput};
use crate::ui::onboarding::{OnboardingOutput, OnboardingWindow};
use crate::ui::preferences::accounts_page::{AccountsPage, AccountsPageOutput};
use crate::ui::preferences::appearance_page::{AppearancePage, AppearancePageOutput};
use crate::ui::preferences::chat_page::{ChatPage, ChatPageOutput};

/// Returned handles from `create_preferences_window` so the caller can store them.
pub struct PreferencesHandles {
    pub window: adw::PreferencesWindow,
    pub accounts_page: Controller<AccountsPage>,
    pub chat_page: Controller<ChatPage>,
    pub appearance_page: Controller<AppearancePage>,
}

pub fn create_preferences_window(
    parent: &adw::ApplicationWindow,
    sender: &relm4::Sender<AppMsg>,
    db: &Database,
    settings: &AppSettings,
) -> PreferencesHandles {
    let accounts = {
        let conn = db.conn_ref().lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, provider, label, api_base_url, default_model, is_default, status, total_tokens_in, total_tokens_out, created_at, updated_at FROM accounts ORDER BY provider, label",
            )
            .unwrap_or_else(|_| panic!("Failed to prepare accounts query"));
        stmt.query_map([], |row| Ok(Database::row_to_account_pub(row)))
            .unwrap()
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>()
    };

    let accounts_page = AccountsPage::builder().launch(accounts).forward(
        sender,
        |output| match output {
            AccountsPageOutput::AddAccount => AppMsg::OpenAccountSetup,
            AccountsPageOutput::DeleteAccount(id) => AppMsg::DeleteAccountFromPrefs(id),
        },
    );

    let chat_page = ChatPage::builder()
        .launch(settings.clone())
        .forward(sender, |output| match output {
            ChatPageOutput::SettingsChanged(s) => AppMsg::SettingsChanged(s),
        });

    let appearance_page = AppearancePage::builder()
        .launch(settings.clone())
        .forward(sender, |output| match output {
            AppearancePageOutput::SettingsChanged(s) => AppMsg::SettingsChanged(s),
        });

    let prefs_window = adw::PreferencesWindow::new();
    prefs_window.set_title(Some("Preferences"));
    prefs_window.set_transient_for(Some(parent));
    prefs_window.set_modal(true);
    prefs_window.add(chat_page.widget());
    prefs_window.add(appearance_page.widget());
    prefs_window.add(accounts_page.widget());

    prefs_window.present();

    PreferencesHandles {
        window: prefs_window,
        accounts_page,
        chat_page,
        appearance_page,
    }
}

pub fn create_shortcuts_window(parent: &adw::ApplicationWindow) {
    let window = gtk::ShortcutsWindow::builder()
        .transient_for(parent)
        .modal(true)
        .build();

    // General section
    let general_group = gtk::ShortcutsGroup::builder()
        .title("General")
        .build();

    let new_chat = gtk::ShortcutsShortcut::builder()
        .title("New chat")
        .accelerator("<Control>n")
        .build();
    general_group.add_shortcut(&new_chat);

    let prefs = gtk::ShortcutsShortcut::builder()
        .title("Preferences")
        .accelerator("<Control>comma")
        .build();
    general_group.add_shortcut(&prefs);

    let shortcuts_help = gtk::ShortcutsShortcut::builder()
        .title("Keyboard shortcuts")
        .accelerator("<Control>slash")
        .build();
    general_group.add_shortcut(&shortcuts_help);

    let quick_switch = gtk::ShortcutsShortcut::builder()
        .title("Quick account/model switch")
        .accelerator("<Control>k")
        .build();
    general_group.add_shortcut(&quick_switch);

    // Chat section
    let chat_group = gtk::ShortcutsGroup::builder()
        .title("Chat")
        .build();

    let send = gtk::ShortcutsShortcut::builder()
        .title("Send message")
        .accelerator("Return")
        .build();
    chat_group.add_shortcut(&send);

    let newline = gtk::ShortcutsShortcut::builder()
        .title("New line")
        .accelerator("<Shift>Return")
        .build();
    chat_group.add_shortcut(&newline);

    let find = gtk::ShortcutsShortcut::builder()
        .title("Search in conversation")
        .accelerator("<Control>f")
        .build();
    chat_group.add_shortcut(&find);

    let stop = gtk::ShortcutsShortcut::builder()
        .title("Stop generation")
        .accelerator("Escape")
        .build();
    chat_group.add_shortcut(&stop);

    let paste = gtk::ShortcutsShortcut::builder()
        .title("Paste image from clipboard")
        .accelerator("<Control>v")
        .build();
    chat_group.add_shortcut(&paste);

    let section = gtk::ShortcutsSection::builder()
        .title("Echo")
        .build();
    section.add_group(&general_group);
    section.add_group(&chat_group);

    window.add_section(&section);
    window.present();
}

pub fn create_about_dialog(parent: &adw::ApplicationWindow) {
    let about = adw::AboutWindow::builder()
        .application_name(config::APP_NAME)
        .version(config::VERSION)
        .developer_name("Echo Contributors")
        .license_type(gtk::License::Gpl30)
        .comments("A native Linux desktop AI chat application for GNOME")
        .application_icon("com.echo.Echo")
        .build();
    about.set_transient_for(Some(parent));
    about.present();
}

pub fn create_account_setup(
    parent: &adw::ApplicationWindow,
    sender: &relm4::Sender<AppMsg>,
    provider: ProviderId,
) -> AsyncController<AccountSetupDialog> {
    let setup = AccountSetupDialog::builder()
        .launch(provider)
        .forward(sender, |output| match output {
            AccountSetupOutput::AccountAdded {
                provider,
                label,
                api_key,
                base_url,
                default_model,
                set_as_default,
            } => AppMsg::AccountAdded {
                provider,
                label,
                api_key,
                base_url,
                default_model,
                set_as_default,
            },
            AccountSetupOutput::Cancelled => AppMsg::AccountSetupCancelled,
        });

    setup.widget().set_transient_for(Some(parent));
    setup.widget().present();

    setup
}

pub fn create_onboarding(
    parent: &adw::ApplicationWindow,
    sender: &relm4::Sender<AppMsg>,
) -> AsyncController<OnboardingWindow> {
    let onboarding = OnboardingWindow::builder().launch(()).forward(
        sender,
        |output| match output {
            OnboardingOutput::SetupProvider(provider) => {
                AppMsg::OnboardingSetupProvider(provider)
            }
            OnboardingOutput::Skipped => AppMsg::OnboardingSkipped,
        },
    );

    onboarding.widget().set_transient_for(Some(parent));
    onboarding.widget().present();

    onboarding
}
