use adw::prelude::*;
use relm4::prelude::*;

use crate::services::settings::{AppSettings, ColorScheme, MessageSpacing};

pub struct AppearancePage {
    settings: AppSettings,
}

#[derive(Debug)]
pub enum AppearancePageMsg {
    ColorSchemeChanged(u32),
    MessageSpacingChanged(u32),
    MessageFontSizeChanged(f64),
    CodeFontSizeChanged(f64),
}

#[derive(Debug)]
pub enum AppearancePageOutput {
    SettingsChanged(AppSettings),
}

#[relm4::component(pub)]
impl Component for AppearancePage {
    type Init = AppSettings;
    type Input = AppearancePageMsg;
    type Output = AppearancePageOutput;
    type CommandOutput = ();

    view! {
        adw::PreferencesPage {
            set_title: "Appearance",
            set_icon_name: Some("applications-graphics-symbolic"),

            adw::PreferencesGroup {
                set_title: "Theme",

                #[name = "color_scheme_row"]
                adw::ComboRow {
                    set_title: "Color scheme",
                    set_subtitle: "Choose light, dark, or follow system",
                    set_model: Some(&gtk::StringList::new(&["System", "Light", "Dark"])),
                    set_selected: match model.settings.color_scheme {
                        ColorScheme::System => 0,
                        ColorScheme::Light => 1,
                        ColorScheme::Dark => 2,
                    },
                    connect_selected_notify[sender] => move |row| {
                        sender.input(AppearancePageMsg::ColorSchemeChanged(row.selected()));
                    },
                },
            },

            adw::PreferencesGroup {
                set_title: "Typography",

                adw::ActionRow {
                    set_title: "Message font size",

                    #[name = "msg_font_scale"]
                    add_suffix = &gtk::Scale::with_range(gtk::Orientation::Horizontal, 10.0, 24.0, 1.0) {
                        set_width_request: 200,
                        set_valign: gtk::Align::Center,
                        set_draw_value: true,
                        set_digits: 0,
                        set_value: model.settings.message_font_size as f64,
                        connect_value_changed[sender] => move |scale| {
                            sender.input(AppearancePageMsg::MessageFontSizeChanged(scale.value()));
                        },
                    },
                },

                adw::ActionRow {
                    set_title: "Code font size",

                    #[name = "code_font_scale"]
                    add_suffix = &gtk::Scale::with_range(gtk::Orientation::Horizontal, 10.0, 20.0, 1.0) {
                        set_width_request: 200,
                        set_valign: gtk::Align::Center,
                        set_draw_value: true,
                        set_digits: 0,
                        set_value: model.settings.code_font_size as f64,
                        connect_value_changed[sender] => move |scale| {
                            sender.input(AppearancePageMsg::CodeFontSizeChanged(scale.value()));
                        },
                    },
                },
            },

            adw::PreferencesGroup {
                set_title: "Layout",

                #[name = "spacing_row"]
                adw::ComboRow {
                    set_title: "Message spacing",
                    set_model: Some(&gtk::StringList::new(&["Compact", "Comfortable", "Spacious"])),
                    set_selected: match model.settings.message_spacing {
                        MessageSpacing::Compact => 0,
                        MessageSpacing::Comfortable => 1,
                        MessageSpacing::Spacious => 2,
                    },
                    connect_selected_notify[sender] => move |row| {
                        sender.input(AppearancePageMsg::MessageSpacingChanged(row.selected()));
                    },
                },
            },
        }
    }

    fn init(
        settings: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self { settings };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            AppearancePageMsg::ColorSchemeChanged(idx) => {
                self.settings.color_scheme = match idx {
                    1 => ColorScheme::Light,
                    2 => ColorScheme::Dark,
                    _ => ColorScheme::System,
                };

                // Apply immediately
                apply_color_scheme(self.settings.color_scheme);
                let _ = sender.output(AppearancePageOutput::SettingsChanged(self.settings.clone()));
            }
            AppearancePageMsg::MessageSpacingChanged(idx) => {
                self.settings.message_spacing = match idx {
                    0 => MessageSpacing::Compact,
                    2 => MessageSpacing::Spacious,
                    _ => MessageSpacing::Comfortable,
                };
                let _ = sender.output(AppearancePageOutput::SettingsChanged(self.settings.clone()));
            }
            AppearancePageMsg::MessageFontSizeChanged(val) => {
                self.settings.message_font_size = val as u32;
                let _ = sender.output(AppearancePageOutput::SettingsChanged(self.settings.clone()));
            }
            AppearancePageMsg::CodeFontSizeChanged(val) => {
                self.settings.code_font_size = val as u32;
                let _ = sender.output(AppearancePageOutput::SettingsChanged(self.settings.clone()));
            }
        }
    }
}

pub fn apply_color_scheme(scheme: ColorScheme) {
    let style_manager = adw::StyleManager::default();
    style_manager.set_color_scheme(match scheme {
        ColorScheme::System => adw::ColorScheme::Default,
        ColorScheme::Light => adw::ColorScheme::ForceLight,
        ColorScheme::Dark => adw::ColorScheme::ForceDark,
    });
}
