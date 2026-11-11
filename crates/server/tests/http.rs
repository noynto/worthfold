//! Tests HTTP : le routeur complet, branché sur les providers en mémoire.
//! Ils vérifient la traduction des cas d'usage en réponses : htmx ou non, refus, succès, introuvable.

use std::sync::Arc;

use application::UseCases;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use testing::{InMemoryAssets, InMemoryValuations};
use tower::ServiceExt;

fn app() -> Router {
    worthfold_server::router(Arc::new(UseCases::new(
        Arc::new(InMemoryAssets::default()),
        Arc::new(InMemoryValuations::default()),
    )))
}

/// L'hôte local, comme l'envoie le navigateur sur http://127.0.0.1:3000.
const HOST: &str = "127.0.0.1:3000";

fn get(uri: &str) -> Request<Body> {
    Request::get(uri)
        .header(header::HOST, HOST)
        .body(Body::empty())
        .unwrap()
}

/// Un formulaire envoyé depuis une page de worthfold, par htmx (`htmx = true`) ou par le navigateur seul.
fn post(uri: &str, form: &str, htmx: bool) -> Request<Body> {
    let mut request = Request::post(uri)
        .header(header::HOST, HOST)
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if htmx {
        request = request.header("HX-Request", "true");
    }
    request.body(Body::from(form.to_owned())).unwrap()
}

async fn body(response: Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn header_of<'a>(response: &'a Response<Body>, name: &str) -> &'a str {
    response
        .headers()
        .get(name)
        .map_or("", |v| v.to_str().unwrap())
}

const VALID_ASSET: &str = "name=Livret+A&kind=savings_book&currency=EUR";
const NAMELESS_ASSET: &str = "name=&kind=savings_book&currency=EUR";

#[tokio::test]
async fn les_pages_principales_repondent() {
    for uri in [
        "/",
        "/assets",
        "/fragments/net-worth",
        "/fragments/assets",
        "/fragments/chart",
    ] {
        let response = app().oneshot(get(uri)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
    }
}

#[tokio::test]
async fn avec_htmx_un_refus_repond_422_avec_le_message() {
    let response = app()
        .oneshot(post("/assets", NAMELESS_ASSET, true))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body(response).await.contains("nom"));
}

#[tokio::test]
async fn sans_javascript_un_refus_revient_sur_la_page_avec_le_message() {
    let response = app()
        .oneshot(post("/assets", NAMELESS_ASSET, false))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(header_of(&response, "location").starts_with("/assets?error="));
}

#[tokio::test]
async fn avec_htmx_un_succes_previent_les_fragments_et_confirme() {
    let response = app()
        .oneshot(post("/assets", VALID_ASSET, true))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(header_of(&response, "HX-Trigger"), "worth-changed");
    let html = body(response).await;
    assert!(
        html.contains("snackbar") && html.contains("Livret A"),
        "{html}"
    );
}

#[tokio::test]
async fn sans_javascript_un_succes_redirige_avec_un_message_flash() {
    let response = app()
        .oneshot(post("/assets", VALID_ASSET, false))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_of(&response, "location"), "/assets");
    assert!(header_of(&response, "set-cookie").starts_with("worthfold_flash="));
}

#[tokio::test]
async fn une_valorisation_enregistree_apparait_dans_le_total() {
    let app = app();
    app.clone()
        .oneshot(post("/assets", VALID_ASSET, true))
        .await
        .unwrap();
    let assets = body(app.clone().oneshot(get("/fragments/assets")).await.unwrap()).await;
    let id = assets
        .split("href=\"/assets/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("le lien vers l'actif ajouté");

    let response = app
        .clone()
        .oneshot(post(
            &format!("/assets/{id}/valuations"),
            "amount=1+234,56&valued_on=2026-01-15",
            true,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let total = body(app.oneshot(get("/fragments/net-worth")).await.unwrap()).await;
    assert!(total.contains("1\u{202f}234,56"), "{total}");
}

#[tokio::test]
async fn un_actif_inconnu_affiche_une_page_introuvable() {
    let response = app().oneshot(get("/assets/inconnu")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(body(response).await.contains("Actif introuvable"));
}

#[tokio::test]
async fn une_adresse_inconnue_affiche_une_page_introuvable() {
    let response = app().oneshot(get("/nulle-part")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(body(response).await.contains("Page introuvable"));
}

#[tokio::test]
async fn refuse_une_modification_envoyee_depuis_un_autre_site() {
    let app = app();
    let mut request = post("/assets", VALID_ASSET, false);
    request
        .headers_mut()
        .insert("sec-fetch-site", "cross-site".parse().unwrap());
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Rien n'a été écrit.
    let assets = body(app.oneshot(get("/fragments/assets")).await.unwrap()).await;
    assert!(!assets.contains("Livret A"));
}

#[tokio::test]
async fn un_lien_depuis_un_autre_site_ouvre_la_page() {
    let mut request = get("/");
    request
        .headers_mut()
        .insert("sec-fetch-site", "cross-site".parse().unwrap());
    let response = app().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn refuse_un_hote_inconnu_meme_en_lecture() {
    let mut request = get("/");
    request
        .headers_mut()
        .insert(header::HOST, "piege.example:3000".parse().unwrap());
    let response = app().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
