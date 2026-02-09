mod app;
mod config;
mod models;
mod providers;
mod services;
mod ui;

use gtk::prelude::*;
use relm4::prelude::*;
use tracing_subscriber::EnvFilter;

use app::App;
use config::APP_ID;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_startup(|_| {
        let resource_bytes = glib::Bytes::from_static(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/echo.gresource"
        )));
        let resource =
            gio::Resource::from_data(&resource_bytes).expect("Failed to load GResource");
        gio::resources_register(&resource);

        let icon_theme = gtk::IconTheme::for_display(
            &gtk::gdk::Display::default().expect("Could not get default display"),
        );
        icon_theme.add_resource_path("/com/echo/Echo/icons");

        gtk::Window::set_default_icon_name("com.echo.Echo");

        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/com/echo/Echo/style.css");
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("Could not get default display"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    RelmApp::from_app(app).run_async::<App>(());
}
