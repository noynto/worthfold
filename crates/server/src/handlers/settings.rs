//! La page Réglages (`/settings`) : l'index des pages de configuration, destination du rail de navigation.
//! Elle n'a aucune donnée à lire, donc aucun cas d'usage.

use askama::Template;
use axum::response::Response;

use super::reply::render;

#[derive(Template)]
#[template(path = "settings_page.html")]
struct SettingsPage {
    section: &'static str,
}

pub async fn get_settings() -> Response {
    render(SettingsPage {
        section: "settings",
    })
}
