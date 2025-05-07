use authority::{AuthorityProxy, Subject};
use config::files_init;
use dbus::AuthenticationAgent;
use eyre::{ensure, OptionExt, Result, WrapErr};
use gtk::glib::{self, clone, spawn_future_local};
use state::State;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs::read_to_string;
use tokio::sync::broadcast::channel;
use tracing::level_filters::LevelFilter;
use ui::{create_circular_image, get_conf_data};
use zbus::zvariant::Value;

use gtk::{gio::Cancellable, prelude::*, Builder};
use gtk::{
    Application, ApplicationWindow, Box, Button, DropDown, Label, ListItem, PasswordEntry,
    SignalListItemFactory, StringList,
};
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use zbus::conn;

use crate::config::{ConfFile, SystemConfig};
use crate::events::AuthenticationEvent;

mod authority;
mod config;
mod constants;
mod dbus;
mod events;
mod state;
mod ui;

fn remove_slash_start(out: &str) -> String {
    out.trim_start_matches('/').to_string()
}

fn remove_tilde_start(out: &str) -> String {
    out.trim_start_matches('~').to_string()
}

fn multile_displays(win: ApplicationWindow, password_entry: PasswordEntry, layer: String) {
    glib::timeout_add_local_once(std::time::Duration::from_millis(30), move || {
        let display = gtk::gdk::Display::default().unwrap();
        let monitor_list = display.monitors();
        let count = monitor_list.n_items();
        for i in 0..count {
            let item = monitor_list.item(i).unwrap();
            let monitor = item.downcast::<gtk::gdk::Monitor>().unwrap();

            let dimmer = gtk::Window::builder().build();

            dimmer.init_layer_shell();
            if layer == "top" {
                dimmer.set_layer(Layer::Top);
            } else {
                dimmer.set_layer(Layer::Overlay);
            }
            dimmer.set_exclusive_zone(-1);
            dimmer.set_keyboard_mode(KeyboardMode::None);
            dimmer.set_anchor(Edge::Top, true);
            dimmer.set_anchor(Edge::Left, true);
            dimmer.set_anchor(Edge::Right, true);
            dimmer.set_anchor(Edge::Bottom, true);
            dimmer.add_css_class("dimmer");

            dimmer.set_monitor(Some(&monitor));

            dimmer.present();

            if i == (count - 1) {
                win.present();
                password_entry.grab_focus();
            }

            win.connect_hide(move |_| {
                dimmer.destroy();
            });
        }
    });
}

fn setup_tracing() -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy()
                .add_directive("[start_object_server]=debug".parse()?),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_tracing()?;

    let config: SystemConfig = SystemConfig::from_file()?;

    ensure!(
        Path::new(config.get_helper_path()).exists(),
        "Authentication helper located at {} does not exist.",
        config.get_helper_path()
    );
    tracing::info!(
        "using authentication helper located at {}",
        config.get_helper_path()
    );

    gtk::init()?;

    let application = Application::builder()
        .application_id("com.matysek.Cutekit")
        .build();

    let builder = Builder::from_string(constants::UI_XML);

    let window: ApplicationWindow = ui::get_object(&builder, "window")?;
    let password_entry: PasswordEntry = ui::get_object(&builder, "password-entry")?;
    let cancel_button: Button = ui::get_object(&builder, "cancel-button")?;
    let confirm_button: Button = ui::get_object(&builder, "confirm-button")?;
    let info_label: Label = ui::get_object(&builder, "label-message")?;
    let dropdown: DropDown = ui::get_object(&builder, "identity-dropdown")?;
    let logo_box: Box = ui::get_object(&builder, "logo-box")?;

    let factory = SignalListItemFactory::new();

    // Create and center label
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        let label = Label::new(None);
        label.set_margin_end(4);
        label.set_halign(gtk::Align::Center);
        label.set_valign(gtk::Align::Center);
        label.add_css_class("dropdown");
        item.set_child(Some(&label));
    });

    // Bind string to label
    factory.connect_bind(|_, obj| {
        let item = obj.downcast_ref::<ListItem>().unwrap();

        let label = item
            .child()
            .and_then(|child| child.downcast::<Label>().ok());

        let string_object = item
            .item()
            .and_then(|obj| obj.downcast::<glib::Object>().ok())
            .map(|obj| obj.property::<glib::GString>("string"));

        if let (Some(label), Some(text)) = (label, string_object) {
            label.set_label(text.as_str());
        }
    });

    dropdown.set_factory(Some(&factory));

    let config_path = std::env::var("XDG_CONFIG_HOME")
        .or(std::env::var("HOME").map(|e| e + "/.config"))
        .context("Could not resolve configuration path")?;
    let config_file = format!("{}/cutekit/config.json", config_path);
    files_init()?;

    let conf = ConfFile::new(PathBuf::from(config_file))?;
    let layer = get_conf_data(conf.read(), "layer");

    window.init_layer_shell();
    if layer == "top" {
        window.set_layer(Layer::Top);
    } else {
        window.set_layer(Layer::Overlay);
    }
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    password_entry.grab_focus();

    let home = PathBuf::from(std::env::var("HOME").as_ref().unwrap());
    let mut logo_path_string = get_conf_data(conf.read(), "logo");
    let logo_path;
    if logo_path_string.starts_with('~') {
        logo_path_string = remove_tilde_start(&logo_path_string);
        if logo_path_string.starts_with('/') {
            logo_path_string = remove_slash_start(&logo_path_string);
        }
        logo_path = home.join(logo_path_string);
    } else {
        logo_path = PathBuf::from(logo_path_string);
    }
    if logo_path.exists() && logo_path.is_file() {
        let logo = create_circular_image(&logo_path.to_string_lossy(), 60);
        logo_box.append(&logo);
    }

    let mut css = constants::STATIC_CSS.to_string();
    let css_path = format!("{}/cutekit/style.css", config_path);
    let path = Path::new(&css_path);
    if path.exists() && path.is_file() {
        tracing::info!("loading css stylesheet from {}", css_path);
        let new_css = read_to_string(path).await.as_ref().unwrap().to_string();
        css = format!("{}\n{}", css, new_css);
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&css);
    let display = gtk::gdk::Display::default().ok_or_eyre("Could not get default gtk display.")?;
    gtk::style_context_add_provider_for_display(&display, &provider, 1000);

    application.connect_activate(clone!(
        #[weak]
        window,
        move |app| {
            app.add_window(&window);
        }
    ));

    password_entry.connect_activate(clone!(
        #[weak]
        confirm_button,
        move |_| {
            confirm_button.emit_clicked();
        }
    ));

    let (tx, mut rx) = channel::<AuthenticationEvent>(100);

    // Docs say that there are a couple of options for registering ourselves subject
    // wise. Users are having problems with XDG_SESSION_ID not being
    // set on certain desktop environments, so unix-process seems to be preferred
    // (referencing other implementations)
    let locale = "en_US.UTF-8"; // TODO: Needed?
    let subject_kind = "unix-session".to_string();

    let subject_details = HashMap::from([(
        "session-id".to_string(),
        Value::new(
            std::env::var("XDG_SESSION_ID")
                .context("Could not get XDG session id, make sure that it is set and try again.")?,
        ),
    )]);
    let subject = Subject::new(subject_kind, subject_details);

    application.register(Cancellable::NONE)?;
    application.activate();

    let agent = AuthenticationAgent::new(tx.clone(), config.clone());
    let connection = conn::Builder::system()?
        .serve_at(constants::SELF_OBJECT_PATH, agent)?
        .build()
        .await?;

    let proxy = AuthorityProxy::new(&connection).await?;
    proxy
        .register_authentication_agent(&subject, locale, constants::SELF_OBJECT_PATH)
        .await?;

    tracing::info!("Registered as authentication provider.");

    spawn_future_local(clone!(
        #[weak]
        window,
        #[weak]
        builder,
        async move {
            let mut state = State::new(
                tx.clone(),
                cancel_button.clone(),
                confirm_button.clone(),
                password_entry.clone(),
                window.clone(),
                dropdown.clone(),
            );
            loop {
                let failed_alert = ui::build_fail_alert();

                let event = rx.recv().await.expect("Somehow the channel closed.");
                tracing::debug!("recieved event {:#?}", event);

                match event {
                    AuthenticationEvent::Started {
                        cookie,
                        message,
                        names,
                    } => {
                        let res = state.start_authentication(cookie).unwrap();
                        if !res {
                            continue;
                        }

                        let names = names.iter().map(AsRef::as_ref).collect::<Vec<_>>();
                        let store: StringList = builder.object("identity-dropdown-values").unwrap();
                        store.splice(0, store.n_items(), &names);
                        info_label.set_label(&message);

                        tracing::debug!("Attempting to prompt user for authentication.");
                        multile_displays(window.clone(), password_entry.clone(), layer.clone());
                    }
                    AuthenticationEvent::Canceled { cookie: c } => {
                        state.end_authentication(&c);
                    }
                    AuthenticationEvent::UserCanceled { cookie: c } => {
                        state.end_authentication(&c);
                    }
                    AuthenticationEvent::UserProvidedPassword {
                        cookie: c,
                        username: _,
                        password: _,
                    } => {
                        state.end_authentication(&c);
                    }
                    AuthenticationEvent::AuthorizationFailed { cookie: c } => {
                        state.end_authentication(&c);
                        failed_alert.show(Some(&window));
                    }
                }
            }
        }
    ));

    application.run();

    Ok(())
}
