use adw::prelude::*;
use relm4::prelude::*;

use crate::models::Account;

pub struct AccountsPage {
    accounts: Vec<Account>,
    list_box: gtk::ListBox,
}

#[derive(Debug)]
pub enum AccountsPageMsg {
    SetAccounts(Vec<Account>),
    AddAccount,
    DeleteAccount(String),
}

#[derive(Debug)]
pub enum AccountsPageOutput {
    AddAccount,
    DeleteAccount(String),
}

#[relm4::component(pub)]
impl Component for AccountsPage {
    type Init = Vec<Account>;
    type Input = AccountsPageMsg;
    type Output = AccountsPageOutput;
    type CommandOutput = ();

    view! {
        adw::PreferencesPage {
            set_title: "Accounts",
            set_icon_name: Some("system-users-symbolic"),

            adw::PreferencesGroup {
                set_title: "AI Providers",
                set_description: Some("Manage your AI service accounts"),

                #[wrap(Some)]
                set_header_suffix = &gtk::Button {
                    set_icon_name: "list-add-symbolic",
                    set_tooltip_text: Some("Add Account"),
                    add_css_class: "flat",
                    connect_clicked => AccountsPageMsg::AddAccount,
                },

                #[local_ref]
                list_box -> gtk::ListBox {
                    set_selection_mode: gtk::SelectionMode::None,
                    add_css_class: "boxed-list",
                },
            },
        }
    }

    fn init(
        accounts: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list_box = gtk::ListBox::new();

        let model = Self {
            accounts: accounts.clone(),
            list_box: list_box.clone(),
        };

        let widgets = view_output!();

        // Populate the list
        model.rebuild_list(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            AccountsPageMsg::SetAccounts(accounts) => {
                self.accounts = accounts;
                self.rebuild_list(&sender);
            }
            AccountsPageMsg::AddAccount => {
                let _ = sender.output(AccountsPageOutput::AddAccount);
            }
            AccountsPageMsg::DeleteAccount(id) => {
                let _ = sender.output(AccountsPageOutput::DeleteAccount(id));
            }
        }
    }
}

impl AccountsPage {
    fn rebuild_list(&self, sender: &ComponentSender<Self>) {
        // Remove all children
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        for account in &self.accounts {
            let row = adw::ActionRow::builder()
                .title(&account.label)
                .subtitle(format!(
                    "{} - {}{}",
                    account.provider.display_name(),
                    account.default_model,
                    if account.is_default { " (default)" } else { "" }
                ))
                .build();

            let delete_btn = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .valign(gtk::Align::Center)
                .build();
            delete_btn.add_css_class("flat");
            delete_btn.add_css_class("error");

            let account_id = account.id.clone();
            let sender_clone = sender.input_sender().clone();
            delete_btn.connect_clicked(move |_| {
                sender_clone
                    .send(AccountsPageMsg::DeleteAccount(account_id.clone()))
                    .unwrap();
            });

            row.add_suffix(&delete_btn);
            self.list_box.append(&row);
        }

        if self.accounts.is_empty() {
            let row = adw::ActionRow::builder()
                .title("No accounts configured")
                .subtitle("Click + to add an account")
                .build();
            row.add_css_class("dim-label");
            self.list_box.append(&row);
        }
    }
}
