use adw::prelude::*;
use relm4::prelude::*;

use crate::models::ProviderId;
use crate::providers::local::LocalProvider;
use crate::providers::traits::AiProvider;
use crate::providers::types::ModelInfo;

pub struct AccountSetupDialog {
    provider: ProviderId,
    label: String,
    api_key: String,
    custom_endpoint: String,
    default_model: String,
    set_as_default: bool,
    validating: bool,
    status_message: Option<String>,
    status_is_error: bool,
    updating_model: bool,
    model_dropdown: adw::ComboRow,
    /// For Local provider: discovered models from /v1/models
    discovered_models: Vec<String>,
    /// Whether models have been discovered (Local provider two-step flow)
    models_discovered: bool,
}

#[derive(Debug)]
pub enum AccountSetupMsg {
    Cancel,
    ProviderChanged(u32),
    LabelChanged(String),
    ApiKeyChanged(String),
    EndpointChanged(String),
    ModelChanged(u32),
    SetAsDefaultToggled(bool),
    Validate,
}

#[derive(Debug)]
pub enum AccountSetupCmd {
    ModelsDiscovered(Vec<ModelInfo>),
    DiscoveryFailed(String),
}

#[derive(Debug)]
pub enum AccountSetupOutput {
    AccountAdded {
        provider: ProviderId,
        label: String,
        api_key: String,
        base_url: Option<String>,
        default_model: String,
        set_as_default: bool,
    },
    Cancelled,
}

#[relm4::component(pub, async)]
impl AsyncComponent for AccountSetupDialog {
    type Init = ProviderId;
    type Input = AccountSetupMsg;
    type Output = AccountSetupOutput;
    type CommandOutput = AccountSetupCmd;

    view! {
        adw::Window {
            set_title: Some("Add Account"),
            set_default_width: 450,
            set_default_height: -1,
            set_modal: true,

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    pack_start = &gtk::Button {
                        set_label: "Cancel",
                        connect_clicked => AccountSetupMsg::Cancel,
                    },

                    pack_end = &gtk::Button {
                        #[watch]
                        set_label: if model.provider == ProviderId::Local && !model.models_discovered { "Discover Models" } else { "Save" },
                        add_css_class: "suggested-action",
                        #[watch]
                        set_sensitive: !model.validating && !model.label.is_empty() && model.is_save_enabled(),
                        connect_clicked => AccountSetupMsg::Validate,
                    },
                },

                #[wrap(Some)]
                set_content = &adw::Clamp {
                    set_maximum_size: 400,
                    set_margin_all: 16,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,

                        adw::PreferencesGroup {
                            set_title: "Provider",

                            #[name = "provider_dropdown"]
                            adw::ComboRow {
                                set_title: "Provider",
                                set_model: Some(&Self::provider_list()),
                                connect_selected_notify[sender] => move |row| {
                                    sender.input(AccountSetupMsg::ProviderChanged(row.selected()));
                                },
                            },
                        },

                        adw::PreferencesGroup {
                            set_title: "Account Details",

                            adw::EntryRow {
                                set_title: "Label",
                                connect_changed[sender] => move |entry| {
                                    sender.input(AccountSetupMsg::LabelChanged(entry.text().to_string()));
                                },
                            },

                            #[name = "api_key_row"]
                            adw::PasswordEntryRow {
                                set_title: "API Key",
                                connect_changed[sender] => move |entry| {
                                    sender.input(AccountSetupMsg::ApiKeyChanged(entry.text().to_string()));
                                },
                            },

                            #[name = "endpoint_row"]
                            adw::EntryRow {
                                set_title: "Custom Endpoint (optional)",
                                connect_changed[sender] => move |entry| {
                                    sender.input(AccountSetupMsg::EndpointChanged(entry.text().to_string()));
                                },
                            },

                            #[name = "model_dropdown"]
                            adw::ComboRow {
                                set_title: "Default Model",
                                set_model: Some(&Self::model_list(&model.provider)),
                                connect_selected_notify[sender] => move |row| {
                                    sender.input(AccountSetupMsg::ModelChanged(row.selected()));
                                },
                            },

                            adw::SwitchRow {
                                set_title: "Set as Default",
                                set_active: true,
                                connect_active_notify[sender] => move |row| {
                                    sender.input(AccountSetupMsg::SetAsDefaultToggled(row.is_active()));
                                },
                            },
                        },

                        // Status area
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_visible: model.validating || model.status_message.is_some(),

                            gtk::Spinner {
                                #[watch]
                                set_spinning: model.validating,
                                #[watch]
                                set_visible: model.validating,
                            },

                            gtk::Label {
                                #[watch]
                                set_label: model.status_message.as_deref().unwrap_or(""),
                                #[watch]
                                add_css_class: if model.status_is_error { "error" } else { "success" },
                                set_wrap: true,
                            },
                        },
                    },
                },
            },
        }
    }

    async fn init(
        provider: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let default_model = match provider {
            ProviderId::Gemini => "gemini-2.5-flash".to_string(),
            ProviderId::Claude => "claude-sonnet-4-20250514".to_string(),
            ProviderId::Local => String::new(),
        };

        let model = Self {
            provider,
            label: String::new(),
            api_key: String::new(),
            custom_endpoint: String::new(),
            default_model,
            set_as_default: true,
            validating: false,
            status_message: None,
            status_is_error: false,
            updating_model: false,
            model_dropdown: adw::ComboRow::new(),
            discovered_models: Vec::new(),
            models_discovered: false,
        };

        let widgets = view_output!();

        // Store reference to model dropdown for imperative updates
        let mut model = model;
        model.model_dropdown = widgets.model_dropdown.clone();

        // Set initial provider selection
        let provider_index = match model.provider {
            ProviderId::Gemini => 0,
            ProviderId::Claude => 1,
            ProviderId::Local => 2,
        };
        widgets.provider_dropdown.set_selected(provider_index);

        // Update UI hints for Local provider
        if model.provider == ProviderId::Local {
            widgets.api_key_row.set_title("API Key (optional)");
            widgets.endpoint_row.set_title("Base URL");
        }

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            AccountSetupMsg::Cancel => {
                let _ = sender.output(AccountSetupOutput::Cancelled);
                root.close();
            }
            AccountSetupMsg::ProviderChanged(index) => {
                let new_provider = match index {
                    0 => ProviderId::Gemini,
                    1 => ProviderId::Claude,
                    _ => ProviderId::Local,
                };
                if self.provider != new_provider {
                    self.provider = new_provider;
                    self.default_model = match new_provider {
                        ProviderId::Gemini => "gemini-2.5-flash".to_string(),
                        ProviderId::Claude => "claude-sonnet-4-20250514".to_string(),
                        ProviderId::Local => String::new(),
                    };
                    self.discovered_models.clear();
                    self.models_discovered = false;
                    self.status_message = None;
                    // Update model dropdown imperatively to avoid infinite loop
                    self.updating_model = true;
                    let model_list = Self::model_list(&self.provider);
                    self.model_dropdown.set_model(Some(&model_list));
                    if model_list.n_items() > 0 {
                        self.model_dropdown.set_selected(0);
                    }
                    self.updating_model = false;
                }
            }
            AccountSetupMsg::LabelChanged(label) => {
                self.label = label;
            }
            AccountSetupMsg::ApiKeyChanged(key) => {
                self.api_key = key;
                self.status_message = None;
            }
            AccountSetupMsg::EndpointChanged(endpoint) => {
                self.custom_endpoint = endpoint;
                // Reset discovered models when endpoint changes
                if self.provider == ProviderId::Local {
                    self.discovered_models.clear();
                    self.models_discovered = false;
                    self.status_message = None;
                }
            }
            AccountSetupMsg::ModelChanged(index) => {
                if self.updating_model {
                    return;
                }
                if self.provider == ProviderId::Local && self.models_discovered {
                    if let Some(model) = self.discovered_models.get(index as usize) {
                        self.default_model = model.clone();
                    }
                } else {
                    let models = Self::model_ids(&self.provider);
                    if let Some(model) = models.get(index as usize) {
                        self.default_model = model.clone();
                    }
                }
            }
            AccountSetupMsg::SetAsDefaultToggled(active) => {
                self.set_as_default = active;
            }
            AccountSetupMsg::Validate => {
                if self.provider == ProviderId::Local && !self.models_discovered {
                    // First step: discover models
                    self.validating = true;
                    self.status_message = Some("Discovering models...".to_string());
                    self.status_is_error = false;

                    let api_key = self.api_key.clone();
                    let base_url = self.custom_endpoint.clone();

                    sender.command(|out, _| {
                        Box::pin(async move {
                            let provider = LocalProvider::new();
                            match provider
                                .validate_credentials(&api_key, Some(&base_url))
                                .await
                            {
                                Ok(models) => {
                                    out.send(AccountSetupCmd::ModelsDiscovered(models)).unwrap()
                                }
                                Err(e) => out
                                    .send(AccountSetupCmd::DiscoveryFailed(e.to_string()))
                                    .unwrap(),
                            }
                        })
                    });
                } else {
                    // Save the account (cloud providers, or Local after model discovery)
                    self.validating = true;
                    self.status_message = Some("Validating credentials...".to_string());
                    self.status_is_error = false;

                    // Send output BEFORE closing - closing may tear down the component
                    let _ = sender.output(AccountSetupOutput::AccountAdded {
                        provider: self.provider,
                        label: self.label.clone(),
                        api_key: self.api_key.clone(),
                        base_url: if self.custom_endpoint.is_empty() {
                            None
                        } else {
                            Some(self.custom_endpoint.clone())
                        },
                        default_model: self.default_model.clone(),
                        set_as_default: self.set_as_default,
                    });
                    root.close();
                }
            }
        }
    }

    async fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        _sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            AccountSetupCmd::ModelsDiscovered(models) => {
                self.validating = false;
                if models.is_empty() {
                    self.status_message = Some("No models found at this endpoint".to_string());
                    self.status_is_error = true;
                    return;
                }

                self.discovered_models = models.iter().map(|m| m.id.clone()).collect();
                self.models_discovered = true;
                self.default_model = self.discovered_models.first().cloned().unwrap_or_default();

                // Populate model dropdown with discovered models
                self.updating_model = true;
                let names: Vec<&str> = self.discovered_models.iter().map(|s| s.as_str()).collect();
                let list = gtk::StringList::new(&names);
                self.model_dropdown.set_model(Some(&list));
                self.model_dropdown.set_selected(0);
                self.updating_model = false;

                self.status_message = Some(format!(
                    "Found {} model{}",
                    self.discovered_models.len(),
                    if self.discovered_models.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
                self.status_is_error = false;
            }
            AccountSetupCmd::DiscoveryFailed(error) => {
                self.validating = false;
                self.status_message = Some(error);
                self.status_is_error = true;
            }
        }
    }
}

impl AccountSetupDialog {
    fn provider_list() -> gtk::StringList {
        gtk::StringList::new(&[
            "Google Gemini",
            "Anthropic Claude",
            "Local (OpenAI Compatible)",
        ])
    }

    fn model_list(provider: &ProviderId) -> gtk::StringList {
        let models = Self::model_names(provider);
        let refs: Vec<&str> = models.iter().map(|s| s.as_str()).collect();
        gtk::StringList::new(&refs)
    }

    fn model_names(provider: &ProviderId) -> Vec<String> {
        match provider {
            ProviderId::Gemini => vec![
                "Gemini 2.5 Flash".to_string(),
                "Gemini 2.5 Pro".to_string(),
                "Gemini 2.0 Flash".to_string(),
            ],
            ProviderId::Claude => vec![
                "Claude Sonnet 4".to_string(),
                "Claude Opus 4".to_string(),
                "Claude 3.5 Haiku".to_string(),
            ],
            ProviderId::Local => vec![],
        }
    }

    fn model_ids(provider: &ProviderId) -> Vec<String> {
        match provider {
            ProviderId::Gemini => vec![
                "gemini-2.5-flash".to_string(),
                "gemini-2.5-pro".to_string(),
                "gemini-2.0-flash".to_string(),
            ],
            ProviderId::Claude => vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-20250514".to_string(),
                "claude-haiku-3-5-20241022".to_string(),
            ],
            ProviderId::Local => vec![],
        }
    }

    fn is_save_enabled(&self) -> bool {
        match self.provider {
            ProviderId::Local => {
                if self.models_discovered {
                    !self.default_model.is_empty()
                } else {
                    !self.custom_endpoint.is_empty()
                }
            }
            _ => !self.api_key.is_empty(),
        }
    }
}
