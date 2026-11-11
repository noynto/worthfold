//! Fichiers statiques embarqués dans le binaire : la feuille de style compilée, la police et htmx.

use axum::http::header;
use axum::response::IntoResponse;

const APP_CSS: &str = include_str!(concat!(env!("OUT_DIR"), "/app.css"));
const ROBOTO_FLEX: &[u8] = include_bytes!("../assets/fonts/roboto-flex-latin.woff2");
/// htmx 2.0.11 (licence 0BSD, `assets/js/htmx-LICENSE.txt`). La version est dans le chemin : cache permanent.
const HTMX: &str = include_str!("../assets/js/htmx-2.0.11.min.js");

pub async fn app_css() -> impl IntoResponse {
    // `no-cache` : le navigateur revalide, et voit la nouvelle feuille après un nouveau build.
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        APP_CSS,
    )
}

pub async fn roboto_flex() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "font/woff2"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        ROBOTO_FLEX,
    )
}

pub async fn htmx() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        HTMX,
    )
}
