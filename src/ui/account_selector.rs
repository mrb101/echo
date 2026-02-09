use std::collections::HashMap;

use gtk::prelude::*;
use relm4::prelude::*;

use crate::models::Account;

pub struct AccountSelector {
    accounts: Vec<Account>,
    selected_account_index: Option<usize>,
    models: Vec<String>,
    selected_model_index: Option<usize>,
    account_dropdown: gtk::DropDown,
    model_dropdown: gtk::DropDown,
    /// Guard flag to prevent infinite loop from DropDown set_model -> selected-notify -> update cycle
    updating: bool,
    /// Discovered models for Local provider accounts, keyed by account ID
    local_models: HashMap<String, Vec<String>>,
}

#[derive(Debug)]
pub enum AccountSelectorMsg {
    SetAccounts(Vec<Account>),
    AccountChanged(u32),
    ModelChanged(u32),
    SyncToConversation(String, String), // (account_id, model)
    FinishSync,
    SetLocalModels(String, Vec<String>), // (account_id, model_ids)
    GrabFocus,
}

#[derive(Debug)]
pub enum AccountSelectorOutput {
    AccountSelected(String),
    ModelSelected(String),
}

#[relm4::component(pub)]
impl Component for AccountSelector {
    type Init = ();
    type Input = AccountSelectorMsg;
    type Output = AccountSelectorOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,

            #[local_ref]
            account_dropdown -> gtk::DropDown {},

            #[local_ref]
            model_dropdown -> gtk::DropDown {},
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let account_dropdown = gtk::DropDown::builder()
            .tooltip_text("Select account")
            .build();
        account_dropdown.add_css_class("flat");

        let model_dropdown = gtk::DropDown::builder()
            .tooltip_text("Select model")
            .build();
        model_dropdown.add_css_class("flat");

        let model = Self {
            accounts: Vec::new(),
            selected_account_index: None,
            models: Vec::new(),
            selected_model_index: None,
            account_dropdown: account_dropdown.clone(),
            model_dropdown: model_dropdown.clone(),
            updating: false,
            local_models: HashMap::new(),
        };

        let widgets = view_output!();

        // Connect account dropdown
        let sender_acc = sender.clone();
        account_dropdown.connect_selected_notify(move |dd| {
            sender_acc.input(AccountSelectorMsg::AccountChanged(dd.selected()));
        });

        // Connect model dropdown
        let sender_model = sender.clone();
        model_dropdown.connect_selected_notify(move |dd| {
            sender_model.input(AccountSelectorMsg::ModelChanged(dd.selected()));
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            AccountSelectorMsg::SetAccounts(accounts) => {
                self.updating = true;
                self.accounts = accounts;
                if !self.accounts.is_empty() {
                    // Select the default account or the first one
                    let default_idx = self
                        .accounts
                        .iter()
                        .position(|a| a.is_default)
                        .unwrap_or(0);
                    self.selected_account_index = Some(default_idx);
                    self.update_models_for_account(default_idx);

                    // Update dropdown models imperatively (not via #[watch])
                    self.sync_account_dropdown(default_idx);
                    self.sync_model_dropdown(0);

                    let _ = sender.output(AccountSelectorOutput::AccountSelected(
                        self.accounts[default_idx].id.clone(),
                    ));
                    if let Some(model) = self.models.first() {
                        let _ = sender.output(AccountSelectorOutput::ModelSelected(
                            model.clone(),
                        ));
                    }
                } else {
                    self.selected_account_index = None;
                    self.models.clear();
                    self.selected_model_index = None;
                    self.account_dropdown.set_model(None::<&gtk::StringList>);
                    self.model_dropdown.set_model(None::<&gtk::StringList>);
                }
                // Defer clearing the updating flag: DropDown set_model/set_selected
                // fire selected-notify synchronously, queuing AccountChanged/ModelChanged
                // messages. Those must be processed while `updating` is still true.
                // FinishSync is enqueued after them, so it runs last.
                sender.input(AccountSelectorMsg::FinishSync);
            }
            AccountSelectorMsg::AccountChanged(index) => {
                if self.updating {
                    return;
                }
                let index = index as usize;
                if index < self.accounts.len() && self.selected_account_index != Some(index) {
                    self.updating = true;
                    self.selected_account_index = Some(index);
                    self.update_models_for_account(index);
                    self.sync_model_dropdown(0);
                    // Defer clearing updating flag (see SetAccounts comment)
                    sender.input(AccountSelectorMsg::FinishSync);
                    let _ = sender.output(AccountSelectorOutput::AccountSelected(
                        self.accounts[index].id.clone(),
                    ));
                    // Emit the default model for the new account since the
                    // ModelChanged signal was suppressed by the updating guard
                    if let Some(model) = self.models.first() {
                        let _ = sender.output(AccountSelectorOutput::ModelSelected(
                            model.clone(),
                        ));
                    }
                }
            }
            AccountSelectorMsg::ModelChanged(index) => {
                if self.updating {
                    return;
                }
                let index = index as usize;
                if index < self.models.len() && self.selected_model_index != Some(index) {
                    self.selected_model_index = Some(index);
                    let _ = sender.output(AccountSelectorOutput::ModelSelected(
                        self.models[index].clone(),
                    ));
                }
            }
            AccountSelectorMsg::SyncToConversation(account_id, model) => {
                self.updating = true;
                // Find account by ID
                if let Some(acc_idx) = self.accounts.iter().position(|a| a.id == account_id) {
                    self.selected_account_index = Some(acc_idx);
                    self.sync_account_dropdown(acc_idx);
                    self.update_models_for_account(acc_idx);
                    // Find the model in the list
                    let model_idx = self
                        .models
                        .iter()
                        .position(|m| m == &model)
                        .unwrap_or(0);
                    self.selected_model_index = Some(model_idx);
                    self.sync_model_dropdown(model_idx);
                }
                // Defer clearing the updating flag (see SetAccounts comment)
                sender.input(AccountSelectorMsg::FinishSync);
            }
            AccountSelectorMsg::FinishSync => {
                self.updating = false;
            }
            AccountSelectorMsg::GrabFocus => {
                self.account_dropdown.grab_focus();
            }
            AccountSelectorMsg::SetLocalModels(account_id, models) => {
                self.local_models.insert(account_id.clone(), models);
                // If this is the currently selected account, refresh the model dropdown
                if let Some(idx) = self.selected_account_index {
                    if let Some(account) = self.accounts.get(idx) {
                        if account.id == account_id
                            && account.provider == crate::models::ProviderId::Local
                        {
                            let current_model = self
                                .selected_model_index
                                .and_then(|i| self.models.get(i).cloned());
                            self.updating = true;
                            self.update_models_for_account(idx);
                            // Try to preserve the current selection
                            let model_idx = current_model
                                .and_then(|m| self.models.iter().position(|x| x == &m))
                                .unwrap_or(0);
                            self.selected_model_index = Some(model_idx);
                            self.sync_model_dropdown(model_idx);
                            sender.input(AccountSelectorMsg::FinishSync);
                        }
                    }
                }
            }
        }
    }
}

impl AccountSelector {
    fn update_models_for_account(&mut self, account_index: usize) {
        if let Some(account) = self.accounts.get(account_index) {
            // Use the default model and well-known models for the provider
            self.models = vec![account.default_model.clone()];
            // Add other known models if they're different
            let known = match account.provider {
                crate::models::ProviderId::Gemini => {
                    vec![
                        "gemini-2.5-pro".to_string(),
                        "gemini-2.5-flash".to_string(),
                        "gemini-2.0-flash".to_string(),
                    ]
                }
                crate::models::ProviderId::Claude => {
                    vec![
                        "claude-sonnet-4-20250514".to_string(),
                        "claude-opus-4-20250514".to_string(),
                        "claude-haiku-3-5-20241022".to_string(),
                    ]
                }
                crate::models::ProviderId::Local => {
                    self.local_models
                        .get(&account.id)
                        .cloned()
                        .unwrap_or_default()
                }
            };
            for m in known {
                if !self.models.contains(&m) {
                    self.models.push(m);
                }
            }
            self.selected_model_index = Some(0);
        }
    }

    fn sync_account_dropdown(&self, selected: usize) {
        let labels: Vec<&str> = self.accounts.iter().map(|a| a.label.as_str()).collect();
        let list = gtk::StringList::new(&labels);
        self.account_dropdown.set_model(Some(&list));
        self.account_dropdown.set_selected(selected as u32);
    }

    fn sync_model_dropdown(&self, selected: usize) {
        let refs: Vec<&str> = self.models.iter().map(|s| s.as_str()).collect();
        let list = gtk::StringList::new(&refs);
        self.model_dropdown.set_model(Some(&list));
        self.model_dropdown.set_selected(selected as u32);
    }
}
