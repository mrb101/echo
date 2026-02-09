use adw::prelude::*;
use relm4::prelude::*;

use crate::models::ProviderId;

pub struct OnboardingWindow {}

#[derive(Debug)]
pub enum OnboardingMsg {
    SelectProvider(ProviderId),
    Skip,
}

#[derive(Debug)]
pub enum OnboardingOutput {
    SetupProvider(ProviderId),
    Skipped,
}

#[relm4::component(pub, async)]
impl AsyncComponent for OnboardingWindow {
    type Init = ();
    type Input = OnboardingMsg;
    type Output = OnboardingOutput;
    type CommandOutput = ();

    view! {
        adw::Window {
            set_title: Some("Welcome to Echo"),
            set_default_width: 600,
            set_default_height: 500,
            set_modal: true,
            set_resizable: true,

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    set_show_end_title_buttons: true,
                },

                #[wrap(Some)]
                set_content = &adw::Clamp {
                    set_maximum_size: 440,
                    set_margin_top: 32,
                    set_margin_bottom: 40,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 24,
                        set_valign: gtk::Align::Center,
                        set_halign: gtk::Align::Center,

                        gtk::Image {
                            set_icon_name: Some("chat-symbolic"),
                            set_pixel_size: 96,
                            add_css_class: "dim-label",
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,
                            set_halign: gtk::Align::Center,

                            gtk::Label {
                                set_label: "Welcome to Echo",
                                add_css_class: "title-1",
                            },

                            gtk::Label {
                                set_label: "Connect an AI provider to get started",
                                add_css_class: "dim-label",
                            },
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                            set_halign: gtk::Align::Center,

                            gtk::Button {
                                set_label: "Set up Google Gemini",
                                add_css_class: "suggested-action",
                                add_css_class: "pill",
                                set_hexpand: false,
                                connect_clicked => OnboardingMsg::SelectProvider(ProviderId::Gemini),
                            },

                            gtk::Button {
                                set_label: "Set up Anthropic Claude",
                                add_css_class: "suggested-action",
                                add_css_class: "pill",
                                set_hexpand: false,
                                connect_clicked => OnboardingMsg::SelectProvider(ProviderId::Claude),
                            },

                            gtk::Button {
                                set_label: "Set up Local (OpenAI Compatible)",
                                add_css_class: "suggested-action",
                                add_css_class: "pill",
                                set_hexpand: false,
                                connect_clicked => OnboardingMsg::SelectProvider(ProviderId::Local),
                            },

                            gtk::Button {
                                set_label: "Skip for now",
                                add_css_class: "flat",
                                set_hexpand: false,
                                connect_clicked => OnboardingMsg::Skip,
                            },
                        },
                    },
                },
            },
        }
    }

    async fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let model = Self {};
        let widgets = view_output!();
        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            OnboardingMsg::SelectProvider(provider) => {
                // Send output BEFORE closing - closing may tear down the component
                let _ = sender.output(OnboardingOutput::SetupProvider(provider));
                root.close();
            }
            OnboardingMsg::Skip => {
                let _ = sender.output(OnboardingOutput::Skipped);
                root.close();
            }
        }
    }
}
