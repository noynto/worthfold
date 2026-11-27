//! Tests HTTP : le routeur complet, branché sur les providers en mémoire.
//! Ils vérifient la traduction des cas d'usage en réponses : htmx ou non, refus, succès, introuvable.

use std::sync::Arc;

use application::{AuthPorts, UseCases};
use auth::Session;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use testing::{
    InMemoryAssets, InMemoryBudget, InMemoryCategories, InMemoryRecurrence, InMemoryTransactions,
    InMemoryValuations,
};
use tower::ServiceExt;

/// Le jeton de la session déjà ouverte des tests : toutes les requêtes ci-dessous le présentent, comme un
/// navigateur connecté. Les tests de l'authentification partent, eux, d'une application sans session.
const SESSION_TOKEN: &str = "jeton-de-test";
const PASSWORD: &str = "une phrase de passe";
const SETUP_CODE: &str = "TEST-CODE-0001";
const SESSION_COOKIE: &str = "worthfold_session=jeton-de-test";

fn auth_ports(auth: Arc<testing::InMemoryAuth>) -> AuthPorts {
    AuthPorts {
        provider: auth,
        hasher: Arc::new(testing::PlainHasher),
        setup_code: SETUP_CODE.into(),
    }
}

/// Un mot de passe configuré et une session ouverte avec `SESSION_TOKEN`.
fn signed_in() -> Arc<testing::InMemoryAuth> {
    Arc::new(testing::InMemoryAuth::with(
        PASSWORD,
        [Session::open(SESSION_TOKEN, chrono::Utc::now())],
    ))
}

/// Une banque au format CSV le plus courant : les comptes bancaires des tests s'y rattachent.
fn test_banks() -> Arc<testing::InMemoryInstitutions> {
    Arc::new(testing::InMemoryInstitutions::with([
        institution::Institution {
            name: "Ma banque".into(),
            csv: institution::CsvFormat::suggested(),
            pdf: None,
        },
    ]))
}

fn app() -> Router {
    worthfold_server::router(Arc::new(UseCases::new(
        Arc::new(InMemoryAssets::default()),
        Arc::new(InMemoryValuations::default()),
        Arc::new(InMemoryTransactions::default()),
        Arc::new(InMemoryCategories::default()),
        Arc::new(InMemoryBudget::default()),
        Arc::new(InMemoryRecurrence::default()),
        Arc::new(testing::InMemoryBank::default()),
        test_banks(),
        Arc::new(testing::StubPdfReader::default()),
        None,
        auth_ports(signed_in()),
    )))
}

/// L'hôte local, comme l'envoie le navigateur sur http://127.0.0.1:3000.
const HOST: &str = "127.0.0.1:3000";

fn get(uri: &str) -> Request<Body> {
    Request::get(uri)
        .header(header::HOST, HOST)
        .header(header::COOKIE, SESSION_COOKIE)
        .body(Body::empty())
        .unwrap()
}

/// Un formulaire envoyé depuis une page de worthfold, par htmx (`htmx = true`) ou par le navigateur seul.
fn post(uri: &str, form: &str, htmx: bool) -> Request<Body> {
    let mut request = Request::post(uri)
        .header(header::HOST, HOST)
        .header(header::COOKIE, SESSION_COOKIE)
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
async fn chaque_reponse_porte_les_en_tetes_de_securite() {
    let app = app();
    let page = app.clone().oneshot(get("/")).await.unwrap();
    let csp = header_of(&page, "content-security-policy").to_owned();
    assert!(csp.contains("script-src 'self'") && csp.contains("frame-ancestors 'self'"));
    // Aucune exception en ligne : ni script, ni style, ni évaluation de code.
    assert!(
        !csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"),
        "{csp}"
    );
    assert_eq!(header_of(&page, "x-content-type-options"), "nosniff");
    assert_eq!(header_of(&page, "x-frame-options"), "SAMEORIGIN");
    assert_eq!(header_of(&page, "cache-control"), "no-store");

    // Les fichiers statiques gardent leur règle de cache ; une requête refusée reçoit aussi les en-têtes.
    let css = app.clone().oneshot(get("/static/app.css")).await.unwrap();
    assert_eq!(header_of(&css, "cache-control"), "no-cache");
    assert_eq!(header_of(&css, "x-content-type-options"), "nosniff");
    let mut refused = get("/");
    refused
        .headers_mut()
        .insert(header::HOST, "piege.example".parse().unwrap());
    let refused = app.oneshot(refused).await.unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert!(!header_of(&refused, "content-security-policy").is_empty());
}

#[tokio::test]
async fn les_pages_n_ont_ni_script_ni_style_en_ligne() {
    // La CSP les interdirait : un script ou un style en ligne ne s'exécuterait pas, et la page casserait en silence.
    let app = app();
    for uri in [
        "/",
        "/assets",
        "/budget",
        "/payments",
        "/forecast",
        "/bank",
        "/categories",
        "/settings",
        "/account",
        "/fragments/chart",
    ] {
        let page = body(app.clone().oneshot(get(uri)).await.unwrap()).await;
        for forbidden in ["<style", " style=", "hx-on", " onclick=", "javascript:"] {
            assert!(!page.contains(forbidden), "{uri} contient {forbidden}");
        }
        // Les seuls scripts sont ceux de l'application, chargés par adresse.
        let inline_scripts =
            page.matches("<script").count() - page.matches("<script src=\"/static/").count();
        assert_eq!(inline_scripts, 0, "{uri} a un script en ligne");
    }
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

// ── Opérations d'un compte bancaire ──────────────────────────────────────────

const BOUNDARY: &str = "worthfold-test-boundary";

/// L'envoi du formulaire d'import, tel que le navigateur l'encode (`multipart/form-data`).
fn import(uri: &str, statement: &str, closing_balance: &str) -> Request<Body> {
    let body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"statement\"; filename=\"releve.csv\"\r\n\
         Content-Type: text/csv\r\n\r\n{statement}\r\n\
         --{BOUNDARY}\r\nContent-Disposition: form-data; name=\"closing_balance\"\r\n\r\n{closing_balance}\r\n\
         --{BOUNDARY}--\r\n"
    );
    Request::post(uri)
        .header(header::HOST, HOST)
        .header(header::COOKIE, SESSION_COOKIE)
        .header("sec-fetch-site", "same-origin")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

/// Ajoute un actif du type donné et rend son identifiant.
async fn add_asset(app: &Router, kind: &str) -> String {
    app.clone()
        .oneshot(post(
            "/assets",
            &format!("name=Compte&kind={kind}&currency=EUR&bank_name=Ma+banque"),
            true,
        ))
        .await
        .unwrap();
    let assets = body(app.clone().oneshot(get("/fragments/assets")).await.unwrap()).await;
    assets
        .split("href=\"/assets/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("le lien vers l'actif ajouté")
        .to_owned()
}

const STATEMENT: &str =
    "Date;Libellé;Montant\n05/11/2026;PRLV LOYER;-700,00\n06/11/2026;VIR SALAIRE;2 000,00";

#[tokio::test]
async fn un_releve_importe_apparait_dans_les_operations_du_compte() {
    let app = app();
    let id = add_asset(&app, "bank_account").await;

    let response = app
        .clone()
        .oneshot(import(
            &format!("/assets/{id}/transactions/import"),
            STATEMENT,
            "1 300,00",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&response, "location"),
        format!("/assets/{id}/transactions")
    );
    assert!(header_of(&response, "set-cookie").starts_with("worthfold_flash="));

    let page = body(
        app.clone()
            .oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains("PRLV LOYER") && page.contains("VIR SALAIRE"),
        "{page}"
    );
    assert!(page.contains("2 opérations"), "{page}");
    // Chaque opération dit d'où elle vient (règle d'UX 11).
    assert!(page.contains("un relevé</span>"), "{page}"); // l'apostrophe est échappée par askama

    // Le solde indiqué est devenu la valeur du compte.
    let detail = body(app.oneshot(get(&format!("/assets/{id}"))).await.unwrap()).await;
    let compact: String = detail.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.contains("1300,00€"), "{detail}");
    assert!(detail.contains("Opérations du compte"), "{detail}");
}

#[tokio::test]
async fn reimporter_le_meme_releve_ne_double_pas_les_operations() {
    let app = app();
    let id = add_asset(&app, "bank_account").await;
    let uri = format!("/assets/{id}/transactions/import");
    for _ in 0..2 {
        app.clone()
            .oneshot(import(&uri, STATEMENT, ""))
            .await
            .unwrap();
    }
    let page = body(
        app.oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("2 opérations"), "{page}");
}

#[tokio::test]
async fn un_releve_illisible_revient_sur_la_page_avec_le_message() {
    let app = app();
    let id = add_asset(&app, "bank_account").await;
    let response = app
        .oneshot(import(
            &format!("/assets/{id}/transactions/import"),
            "n'importe quoi",
            "",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(header_of(&response, "location").contains("/transactions?error="));
}

#[tokio::test]
async fn l_import_est_refuse_pour_un_actif_qui_n_est_pas_un_compte_bancaire() {
    let app = app();
    let id = add_asset(&app, "savings_book").await;
    let response = app
        .clone()
        .oneshot(import(
            &format!("/assets/{id}/transactions/import"),
            STATEMENT,
            "",
        ))
        .await
        .unwrap();
    assert!(header_of(&response, "location").contains("error="));
    // Et la page d'un livret ne propose pas de lien vers des opérations.
    let detail = body(app.oneshot(get(&format!("/assets/{id}"))).await.unwrap()).await;
    assert!(!detail.contains("Opérations du compte"), "{detail}");
}

#[tokio::test]
async fn les_operations_d_un_compte_inconnu_affichent_une_page_introuvable() {
    let response = app()
        .oneshot(get("/assets/inconnu/transactions"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn l_import_est_refuse_depuis_un_autre_site() {
    let mut request = import("/assets/x/transactions/import", STATEMENT, "");
    request
        .headers_mut()
        .insert("sec-fetch-site", "cross-site".parse().unwrap());
    let response = app().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ── Catégories ───────────────────────────────────────────────────────────────

/// Le premier identifiant trouvé après `prefix` dans une page (`href="/…/"`, `action="…"`).
fn id_after(html: &str, prefix: &str) -> String {
    html.split(prefix)
        .nth(1)
        .and_then(|rest| rest.split(['"', '/']).next())
        .unwrap_or_else(|| panic!("« {prefix} » introuvable dans la page"))
        .to_owned()
}

#[tokio::test]
async fn la_page_categories_propose_les_categories_usuelles_puis_les_cree() {
    let app = app();
    let page = body(app.clone().oneshot(get("/categories")).await.unwrap()).await;
    assert!(page.contains("Aucune catégorie pour l'instant"), "{page}");

    let response = app
        .clone()
        .oneshot(post("/categories/defaults", "", false))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_of(&response, "location"), "/categories");

    let page = body(app.clone().oneshot(get("/categories")).await.unwrap()).await;
    assert!(
        page.contains("Courses") && page.contains("Virement interne"),
        "{page}"
    );
    assert!(!page.contains("Aucune catégorie pour l'instant"), "{page}");
}

#[tokio::test]
async fn refuse_une_categorie_sans_nom_ou_en_double() {
    let app = app();
    let response = app
        .clone()
        .oneshot(post("/categories", "name=&kind=expense", false))
        .await
        .unwrap();
    assert!(header_of(&response, "location").starts_with("/categories?error="));

    app.clone()
        .oneshot(post("/categories", "name=Courses&kind=expense", false))
        .await
        .unwrap();
    let response = app
        .oneshot(post("/categories", "name=courses&kind=expense", false))
        .await
        .unwrap();
    assert!(header_of(&response, "location").starts_with("/categories?error="));
}

#[tokio::test]
async fn une_regle_classe_les_operations_importees_et_celles_deja_la() {
    let app = app();
    let id = add_asset(&app, "bank_account").await;
    app.clone()
        .oneshot(post("/categories", "name=Logement&kind=expense", false))
        .await
        .unwrap();
    let page = body(app.clone().oneshot(get("/categories")).await.unwrap()).await;
    let category = id_after(&page, "action=\"/categories/");

    // Une opération déjà importée, avant la règle.
    let uri = format!("/assets/{id}/transactions/import");
    app.clone()
        .oneshot(import(&uri, STATEMENT, ""))
        .await
        .unwrap();
    app.clone()
        .oneshot(post(
            "/categories/rules",
            &format!("category_id={category}&pattern=loyer"),
            false,
        ))
        .await
        .unwrap();
    let before = body(
        app.clone()
            .oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(!before.contains("selected>Logement"), "{before}");

    // « Classer les opérations » applique la règle à l'existant.
    app.clone()
        .oneshot(post("/categories/apply", "", false))
        .await
        .unwrap();
    let after = body(
        app.clone()
            .oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(after.contains("selected>Logement"), "{after}");
    assert_eq!(after.matches("selected>Logement").count(), 1, "{after}");
}

#[tokio::test]
async fn classer_a_la_main_une_operation_puis_supprimer_la_categorie_la_declasse() {
    let app = app();
    let id = add_asset(&app, "bank_account").await;
    app.clone()
        .oneshot(post("/categories", "name=Courses&kind=expense", false))
        .await
        .unwrap();
    let page = body(app.clone().oneshot(get("/categories")).await.unwrap()).await;
    let category = id_after(&page, "action=\"/categories/");
    app.clone()
        .oneshot(import(
            &format!("/assets/{id}/transactions/import"),
            STATEMENT,
            "",
        ))
        .await
        .unwrap();
    let transactions = body(
        app.clone()
            .oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    let transaction = id_after(&transactions, "id=\"t-");

    let response = app
        .clone()
        .oneshot(post(
            &format!("/assets/{id}/transactions/{transaction}/category"),
            &format!("category_id={category}"),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&response, "location"),
        format!("/assets/{id}/transactions#t-{transaction}")
    );
    let page = body(
        app.clone()
            .oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("selected>Courses"), "{page}");

    // Supprimer la catégorie ne supprime pas l'opération : elle redevient sans catégorie.
    app.clone()
        .oneshot(post(&format!("/categories/{category}/delete"), "", false))
        .await
        .unwrap();
    let page = body(
        app.oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains("2 opérations") && !page.contains("Courses"),
        "{page}"
    );
}

#[tokio::test]
async fn classer_une_operation_inconnue_ou_dans_une_categorie_inconnue_est_refuse() {
    let app = app();
    let id = add_asset(&app, "bank_account").await;
    let response = app
        .oneshot(post(
            &format!("/assets/{id}/transactions/inconnue/category"),
            "category_id=inconnue",
            false,
        ))
        .await
        .unwrap();
    assert!(header_of(&response, "location").contains("error="));
}

#[tokio::test]
async fn les_formulaires_de_categories_sont_refuses_depuis_un_autre_site() {
    for uri in ["/categories", "/categories/apply", "/categories/defaults"] {
        let mut request = post(uri, "name=x&kind=expense", false);
        request
            .headers_mut()
            .insert("sec-fetch-site", "cross-site".parse().unwrap());
        let response = app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }
}

// ── Budget ───────────────────────────────────────────────────────────────────

/// Le mois en cours, comme le serveur le lit (« 2026-11 »).
fn current_month() -> String {
    chrono::Local::now().format("%Y-%m").to_string()
}

#[tokio::test]
async fn la_page_budget_repond_pour_le_mois_en_cours_et_un_mois_passe() {
    for uri in [
        "/budget".to_owned(),
        format!("/budget/{}", current_month()),
        "/budget/2020-03".to_owned(),
    ] {
        let response = app().oneshot(get(&uri)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
    }
    let page = body(app().oneshot(get("/budget/2020-03")).await.unwrap()).await;
    assert!(page.contains("mars 2020"), "{page}");
}

#[tokio::test]
async fn un_mois_invalide_affiche_une_page_introuvable() {
    for uri in ["/budget/octobre", "/budget/2026-13"] {
        let response = app().oneshot(get(uri)).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn un_plafond_fixe_apparait_dans_les_enveloppes_puis_se_retire() {
    let app = app();
    app.clone()
        .oneshot(post("/categories", "name=Courses&kind=expense", false))
        .await
        .unwrap();
    let categories = body(app.clone().oneshot(get("/categories")).await.unwrap()).await;
    let category = id_after(&categories, "action=\"/categories/");
    let month = current_month();

    let response = app
        .clone()
        .oneshot(post(
            &format!("/budget/{month}/envelopes"),
            &format!("category_id={category}&limit=400"),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_of(&response, "location"), format!("/budget/{month}"));

    let page = body(
        app.clone()
            .oneshot(get(&format!("/budget/{month}")))
            .await
            .unwrap(),
    )
    .await;
    let compact: String = page.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        page.contains("Courses") && page.contains("<progress"),
        "{page}"
    );
    assert!(compact.contains("400,00"), "{page}");

    app.clone()
        .oneshot(post(
            &format!("/budget/{month}/envelopes/{category}/remove"),
            "",
            false,
        ))
        .await
        .unwrap();
    let page = body(app.oneshot(get(&format!("/budget/{month}"))).await.unwrap()).await;
    assert!(!page.contains("<progress"), "{page}");
}

#[tokio::test]
async fn refuse_un_plafond_illisible_negatif_ou_sur_une_categorie_de_revenu() {
    let app = app();
    app.clone()
        .oneshot(post("/categories", "name=Salaire&kind=income", false))
        .await
        .unwrap();
    app.clone()
        .oneshot(post("/categories", "name=Courses&kind=expense", false))
        .await
        .unwrap();
    let categories = body(app.clone().oneshot(get("/categories")).await.unwrap()).await;
    // Triées par nom : « Courses » puis « Salaire ».
    let courses = id_after(&categories, "action=\"/categories/");
    let income = categories
        .rsplit("action=\"/categories/")
        .next()
        .and_then(|rest| rest.split('/').next())
        .unwrap()
        .to_owned();
    let uri = format!("/budget/{}/envelopes", current_month());

    for form in [
        format!("category_id={courses}&limit=beaucoup"),
        format!("category_id={courses}&limit=-5"),
        format!("category_id={income}&limit=400"),
        "category_id=inconnue&limit=400".to_owned(),
    ] {
        let response = app.clone().oneshot(post(&uri, &form, false)).await.unwrap();
        assert!(
            header_of(&response, "location").contains("?error="),
            "{form} : {}",
            header_of(&response, "location")
        );
    }
}

#[tokio::test]
async fn le_reglage_de_l_epargne_s_enregistre_et_s_affiche() {
    let app = app();
    let book = add_asset(&app, "savings_book").await;
    let uri = format!("/budget/{}/savings", current_month());

    let response = app
        .clone()
        .oneshot(post(
            &uri,
            &format!("destination={book}&ceiling=22+950&cushion=1+500"),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(!header_of(&response, "location").contains("error"));

    let page = body(app.oneshot(get("/budget")).await.unwrap()).await;
    assert!(page.contains("fixé par toi"), "{page}");
    // Sans opération ni solde : rien à verser, et la page le dit.
    assert!(page.contains("Rien à verser"), "{page}");
}

#[tokio::test]
async fn refuse_un_reglage_d_epargne_invalide() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;
    let uri = format!("/budget/{}/savings", current_month());
    for form in [
        // Un compte courant n'est pas un livret.
        format!("destination={account}&ceiling=22950&cushion="),
        format!("destination={account}&ceiling=beaucoup&cushion="),
        format!("destination={account}&ceiling=22950&cushion=beaucoup"),
    ] {
        let response = app.clone().oneshot(post(&uri, &form, false)).await.unwrap();
        assert!(
            header_of(&response, "location").contains("?error="),
            "{form}"
        );
    }
}

#[tokio::test]
async fn les_formulaires_du_budget_sont_refuses_depuis_un_autre_site() {
    let month = current_month();
    for uri in [
        format!("/budget/{month}/envelopes"),
        format!("/budget/{month}/savings"),
        format!("/budget/{month}/envelopes/x/remove"),
    ] {
        let mut request = post(&uri, "category_id=x&limit=1", false);
        request
            .headers_mut()
            .insert("sec-fetch-site", "cross-site".parse().unwrap());
        let response = app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }
}

// ── Paiements prévus ─────────────────────────────────────────────────────────

/// Le formulaire d'un loyer mensuel au compte donné. `overrides` remplace des champs (un champ n'est jamais
/// répété : `serde_urlencoded` refuse les doublons) ; `extra` en ajoute de nouveaux, sous la forme `a=1&b=2`.
fn payment_form(account: &str, overrides: &[(&str, &str)]) -> String {
    let mut fields: Vec<(String, String)> = [
        ("label", "Loyer"),
        ("pattern", "loyer"),
        ("direction", "out"),
        ("account_id", account),
        ("category_id", ""),
        ("schedule", "monthly"),
        ("day", "5"),
        ("amount", "700"),
        ("margin", ""),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    for (key, value) in overrides {
        match fields.iter_mut().find(|(k, _)| k == key) {
            Some(field) => field.1 = (*value).to_owned(),
            None => fields.push(((*key).to_owned(), (*value).to_owned())),
        }
    }
    fields
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

async fn add_payment(app: &Router, form: &str) -> Response<Body> {
    app.clone()
        .oneshot(post("/payments", form, false))
        .await
        .unwrap()
}

#[tokio::test]
async fn la_page_paiements_demande_d_abord_un_compte_bancaire() {
    let page = body(app().oneshot(get("/payments")).await.unwrap()).await;
    assert!(page.contains("Aucune échéance"), "{page}");
    assert!(
        page.contains("il faut d'abord un compte bancaire")
            || page.contains("Il faut d'abord un compte bancaire"),
        "{page}"
    );
}

#[tokio::test]
async fn un_paiement_ajoute_apparait_dans_la_liste_et_le_calendrier() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;

    let response = add_payment(&app, &payment_form(&account, &[])).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_of(&response, "location"), "/payments");
    assert!(header_of(&response, "set-cookie").starts_with("worthfold_flash="));

    let page = body(app.oneshot(get("/payments")).await.unwrap()).await;
    assert!(page.contains("Chaque mois, le 5"), "{page}");
    // Le loyer revient dans les 120 prochains jours : le calendrier le montre.
    assert!(page.contains("Corriger cette échéance"), "{page}");
    assert!(!page.contains("Aucune échéance"), "{page}");
}

#[tokio::test]
async fn chaque_rythme_se_decrit_en_toutes_lettres() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;
    let cases: [(&[(&str, &str)], &str); 4] = [
        (
            &[
                ("pattern", "fermage"),
                ("schedule", "every_n"),
                ("every_n", "3"),
                ("first_month", "2026-02"),
                ("day", "10"),
            ],
            "Tous les 3 mois, le 10, à partir de février 2026",
        ),
        (
            &[
                ("pattern", "impots"),
                ("schedule", "on_months"),
                ("month_from", "11"),
                ("month_to", "2"),
                ("day", "15"),
            ],
            "Chaque année en janvier, février, novembre, décembre, le 15",
        ),
        (
            &[
                ("pattern", "taxe"),
                ("schedule", "once"),
                ("once_date", "2030-12-20"),
            ],
            "Une fois, le 20/12/2030",
        ),
        (
            &[("pattern", "abo"), ("schedule", "monthly"), ("day", "last")],
            "Chaque mois, le dernier jour du mois",
        ),
    ];
    for (overrides, label) in cases {
        let response = add_payment(&app, &payment_form(&account, overrides)).await;
        assert!(
            !header_of(&response, "location").contains("error"),
            "{label}"
        );
        let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
        assert!(page.contains(label), "{label}\n{page}");
    }
}

#[tokio::test]
async fn refuse_un_paiement_invalide_avec_le_message() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;
    let cases: [(&[(&str, &str)], &str); 15] = [
        (&[("label", "+")], "nom vide"),
        (&[("pattern", "")], "mot du libellé vide"),
        (&[("direction", "autre")], "sens inconnu"),
        (&[("schedule", "autre")], "rythme inconnu"),
        (&[("day", "40")], "jour hors du mois"),
        (
            &[
                ("schedule", "every_n"),
                ("every_n", "1"),
                ("first_month", "2026-02"),
            ],
            "rythme d'un mois",
        ),
        (
            &[("schedule", "every_n"), ("every_n", "3")],
            "premier mois manquant",
        ),
        (&[("schedule", "once")], "date manquante"),
        (&[("amount", "beaucoup")], "montant illisible"),
        (&[("amount", "-5")], "montant négatif"),
        (&[("margin", "-1")], "marge négative"),
        (
            &[("window_count", "3")],
            "nombre d'échéances sans date de début",
        ),
        (
            &[
                ("window_from", "2026-05-01"),
                ("window_until", "2026-04-01"),
            ],
            "fin avant le début",
        ),
        (&[("account_id", "inconnu")], "compte inconnu"),
        (&[("category_id", "inconnue")], "catégorie inconnue"),
    ];
    for (overrides, why) in cases {
        let response = add_payment(&app, &payment_form(&account, overrides)).await;
        assert!(
            header_of(&response, "location").contains("?error="),
            "{why} : {}",
            header_of(&response, "location")
        );
    }
    let page = body(app.oneshot(get("/payments")).await.unwrap()).await;
    assert!(page.contains("Aucun paiement pour l'instant"), "{page}");
}

#[tokio::test]
async fn refuse_deux_paiements_reconnus_par_le_meme_mot() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;
    assert!(
        !header_of(
            &add_payment(&app, &payment_form(&account, &[])).await,
            "location"
        )
        .contains("error")
    );
    let twin = add_payment(&app, &payment_form(&account, &[("label", "Autre")])).await;
    assert!(header_of(&twin, "location").contains("?error="));
}

#[tokio::test]
async fn un_nouveau_montant_s_ajoute_sans_effacer_l_ancien() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;
    add_payment(&app, &payment_form(&account, &[])).await;
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    let payment = id_after(&page, "action=\"/payments/");

    let response = app
        .clone()
        .oneshot(post(
            &format!("/payments/{payment}/steps"),
            "from=2030-01-01&amount=720&margin=",
            false,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(!header_of(&response, "location").contains("error"));

    // Le dernier montant est celui affiché pour le paiement.
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    let compact: String = page.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.contains("720,00"), "{page}");

    for form in [
        "from=&amount=720",
        "from=2030-01-01&amount=beaucoup",
        "from=2030-01-01&amount=-1",
    ] {
        let refused = app
            .clone()
            .oneshot(post(
                &format!("/payments/{payment}/steps"),
                &format!("{form}&margin="),
                false,
            ))
            .await
            .unwrap();
        assert!(
            header_of(&refused, "location").contains("?error="),
            "{form}"
        );
    }
}

#[tokio::test]
async fn une_echeance_se_saute_puis_se_retablit() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;
    add_payment(&app, &payment_form(&account, &[])).await;
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    // L'adresse du formulaire d'une échéance : /payments/{id}/occurrences/{date}.
    let marker = "action=\"/payments/";
    let (payment, date) = page
        .split(marker)
        .filter_map(|rest| rest.split('"').next())
        .find_map(|path| {
            let mut parts = path.split("/occurrences/");
            Some((parts.next()?.to_owned(), parts.next()?.to_owned()))
        })
        .expect("une échéance à corriger");
    let uri = format!("/payments/{payment}/occurrences/{date}");

    let response = app
        .clone()
        .oneshot(post(&uri, "action=skip", false))
        .await
        .unwrap();
    assert!(!header_of(&response, "location").contains("error"));
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    assert!(
        page.contains("Sautée") && page.contains("Rétablir"),
        "{page}"
    );

    app.clone()
        .oneshot(post(&uri, "action=clear", false))
        .await
        .unwrap();
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    assert!(!page.contains("Sautée"), "{page}");

    // Un autre montant, puis les refus : date qui n'est pas une échéance, montant illisible, action inconnue.
    app.clone()
        .oneshot(post(&uri, "action=amount&amount=250", false))
        .await
        .unwrap();
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    assert!(page.contains("montant modifié"), "{page}");
    for (path, form) in [
        (
            format!("/payments/{payment}/occurrences/2030-01-06"),
            "action=skip",
        ),
        (uri.clone(), "action=amount&amount=beaucoup"),
        (uri.clone(), "action=autre"),
        (
            format!("/payments/{payment}/occurrences/pas-une-date"),
            "action=skip",
        ),
    ] {
        let refused = app.clone().oneshot(post(&path, form, false)).await.unwrap();
        assert!(
            header_of(&refused, "location").contains("?error="),
            "{path} {form}"
        );
    }
}

#[tokio::test]
async fn un_paiement_se_supprime() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;
    add_payment(&app, &payment_form(&account, &[])).await;
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    let payment = id_after(&page, "action=\"/payments/");

    let response = app
        .clone()
        .oneshot(post(&format!("/payments/{payment}/delete"), "", false))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    assert!(page.contains("Aucun paiement pour l'instant"), "{page}");

    let again = app
        .oneshot(post(&format!("/payments/{payment}/delete"), "", false))
        .await
        .unwrap();
    assert!(header_of(&again, "location").contains("?error="));
}

#[tokio::test]
async fn l_historique_suggere_un_paiement_regulier_qu_un_clic_ajoute() {
    let app = app();
    let account = add_asset(&app, "bank_account").await;
    // Quatre prélèvements mensuels, des années passées : toujours « récents » par rapport au relevé.
    let statement = "Date;Libellé;Montant\n\
        09/08/2020;PRLV NETFLIX;-12,00\n09/09/2020;PRLV NETFLIX;-12,00\n\
        09/10/2020;PRLV NETFLIX;-12,00\n09/11/2020;PRLV NETFLIX;-12,00";
    app.clone()
        .oneshot(import(
            &format!("/assets/{account}/transactions/import"),
            statement,
            "",
        ))
        .await
        .unwrap();

    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    assert!(
        page.contains("Repéré dans tes relevés") && page.contains("Netflix"),
        "{page}"
    );

    let response = app
        .clone()
        .oneshot(post(
            "/payments/suggestions",
            &format!("account_id={account}&pattern=netflix&direction=out"),
            false,
        ))
        .await
        .unwrap();
    assert!(!header_of(&response, "location").contains("error"));
    let page = body(app.clone().oneshot(get("/payments")).await.unwrap()).await;
    assert!(page.contains("Chaque mois, le 9"), "{page}");
    // Ajoutée : elle n'est plus suggérée.
    assert!(!page.contains("Repéré dans tes relevés"), "{page}");

    // Une suggestion inventée est refusée.
    let forged = app
        .oneshot(post(
            "/payments/suggestions",
            &format!("account_id={account}&pattern=inventé&direction=out"),
            false,
        ))
        .await
        .unwrap();
    assert!(header_of(&forged, "location").contains("?error="));
}

#[tokio::test]
async fn les_formulaires_de_paiements_sont_refuses_depuis_un_autre_site() {
    for uri in [
        "/payments",
        "/payments/suggestions",
        "/payments/x/delete",
        "/payments/x/steps",
        "/payments/x/occurrences/2026-11-05",
    ] {
        let mut request = post(uri, "label=x", false);
        request
            .headers_mut()
            .insert("sec-fetch-site", "cross-site".parse().unwrap());
        let response = app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }
}

#[tokio::test]
async fn la_page_previsions_dit_ce_qui_manque_puis_projette_les_echeances() {
    let app = app();
    let page = body(app.clone().oneshot(get("/forecast")).await.unwrap()).await;
    assert!(page.contains("Prévisions"), "{page}");
    assert!(page.contains("Aucun compte courant"), "{page}");
    assert!(page.contains("Aucun paiement prévu"), "{page}");

    let account = add_asset(&app, "bank_account").await;
    let response = add_payment(&app, &payment_form(&account, &[])).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let page = body(app.oneshot(get("/forecast")).await.unwrap()).await;
    assert!(page.contains("1 paiement prévu"), "{page}");
    // Un loyer sans solde de départ ni dépenses variables : le solde attendu passe sous zéro.
    assert!(page.contains("à découvert"), "{page}");
    assert!(page.contains("Détail mois après mois"), "{page}");
    assert!(page.contains("Évolution prévue"), "{page}");
}

fn bank_app() -> (Router, Arc<testing::FakeAggregator>) {
    let aggregator = Arc::new(testing::FakeAggregator::default());
    let app = worthfold_server::router(Arc::new(UseCases::new(
        Arc::new(InMemoryAssets::default()),
        Arc::new(InMemoryValuations::default()),
        Arc::new(InMemoryTransactions::default()),
        Arc::new(InMemoryCategories::default()),
        Arc::new(InMemoryBudget::default()),
        Arc::new(InMemoryRecurrence::default()),
        Arc::new(testing::InMemoryBank::default()),
        test_banks(),
        Arc::new(testing::StubPdfReader::default()),
        Some(aggregator.clone()),
        auth_ports(signed_in()),
    )));
    (app, aggregator)
}

#[tokio::test]
async fn la_page_banque_dit_quand_la_connexion_n_est_pas_configuree() {
    let page = body(app().oneshot(get("/bank")).await.unwrap()).await;
    assert!(page.contains("Connexion non configurée"), "{page}");
    assert!(!page.contains("Connecter une banque"), "{page}");
}

#[tokio::test]
async fn connecter_une_banque_rattacher_un_compte_et_lire_ses_operations() {
    use bank::{RemoteAccount, RemoteSession, RemoteTransaction};
    let (app, aggregator) = bank_app();
    let account = add_asset(&app, "bank_account").await;
    *aggregator.session.lock().unwrap() = Some(RemoteSession {
        session_id: "s1".into(),
        valid_until: chrono::Local::now().date_naive() + chrono::Days::new(90),
        accounts: vec![RemoteAccount {
            uid: "uid-1".into(),
            identity: "h".into(),
            label: "Compte joint (FR76 … 0189)".into(),
            currency: shared::Currency::Eur,
        }],
    });
    *aggregator.transactions.lock().unwrap() = vec![RemoteTransaction {
        booked_on: chrono::Local::now().date_naive(),
        amount: rust_decimal::Decimal::from(-42),
        currency: shared::Currency::Eur,
        label: "CARTE MARCHE".into(),
    }];

    // Départ : la banque reçoit l'utilisateur, avec un état.
    let response = app
        .clone()
        .oneshot(post(
            "/bank/connect",
            "bank_name=Ma+Banque&country=fr",
            false,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = header_of(&response, "location");
    assert!(
        location.starts_with("https://bank.example/FR/Ma Banque"),
        "{location}"
    );
    let state = location.split("state=").nth(1).unwrap().to_owned();

    // Un retour forgé est refusé ; le vrai retour crée la connexion, et ne sert qu'une fois.
    let forged = app
        .clone()
        .oneshot(get("/bank/callback?code=c&state=forge"))
        .await
        .unwrap();
    assert!(
        header_of(&forged, "location").starts_with("/bank?error="),
        "forgé"
    );
    let back = app
        .clone()
        .oneshot(get(&format!("/bank/callback?code=c&state={state}")))
        .await
        .unwrap();
    assert_eq!(header_of(&back, "location"), "/bank");
    let again = app
        .clone()
        .oneshot(get(&format!("/bank/callback?code=c&state={state}")))
        .await
        .unwrap();
    assert!(
        header_of(&again, "location").starts_with("/bank?error="),
        "rejoué"
    );

    let page = body(app.clone().oneshot(get("/bank")).await.unwrap()).await;
    assert!(page.contains("Compte joint"), "{page}");
    assert!(!page.contains("Lire maintenant"), "{page}");

    // Rattacher le compte, puis lire.
    let link = app
        .clone()
        .oneshot(post(
            "/bank/accounts/uid-1/link",
            &format!("asset_id={account}"),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(header_of(&link, "location"), "/bank");
    let page = body(app.clone().oneshot(get("/bank")).await.unwrap()).await;
    assert!(page.contains("Lire maintenant"), "{page}");

    let sync = app
        .clone()
        .oneshot(post("/bank/sync", "", false))
        .await
        .unwrap();
    assert_eq!(sync.status(), StatusCode::SEE_OTHER);
    let operations = body(
        app.oneshot(get(&format!("/assets/{account}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(operations.contains("CARTE MARCHE"), "{operations}");
}

#[tokio::test]
async fn un_refus_de_la_banque_ne_cree_rien() {
    let (app, _) = bank_app();
    let response = app
        .oneshot(get(
            "/bank/callback?error=access_denied&error_description=Bad%3Cscript%3E",
        ))
        .await
        .unwrap();
    let location = header_of(&response, "location");
    assert!(location.starts_with("/bank?error="), "{location}");
    assert!(!location.contains("script"), "{location}");
}

#[tokio::test]
async fn le_champ_banque_propose_les_banques_du_pays() {
    let (app, aggregator) = bank_app();
    *aggregator.institutions.lock().unwrap() = vec!["BBVA".into(), "Genome <b>".into()];

    let options = body(
        app.clone()
            .oneshot(get("/fragments/banks?country=fr"))
            .await
            .unwrap(),
    )
    .await;
    assert!(options.contains(r#"<option value="BBVA">"#), "{options}");
    // Un nom est une valeur comme une autre : échappé.
    assert!(
        options.contains("Genome &#60;b&#62;") || options.contains("Genome &lt;b&gt;"),
        "{options}"
    );

    // Un pays incomplet ne propose rien ; la page garde le champ libre.
    let none = body(
        app.clone()
            .oneshot(get("/fragments/banks?country=f"))
            .await
            .unwrap(),
    )
    .await;
    assert!(none.trim().is_empty(), "{none}");
    let page = body(app.oneshot(get("/bank")).await.unwrap()).await;
    assert!(page.contains(r#"list="bank-choices""#), "{page}");
}

// ── Authentification ─────────────────────────────────────────────────────────

/// Une application sans mot de passe (premier lancement), ou avec un mot de passe mais sans session ouverte.
fn anonymous_app(configured: bool) -> (Router, Arc<testing::InMemoryAuth>) {
    let auth = Arc::new(if configured {
        testing::InMemoryAuth::with(PASSWORD, [])
    } else {
        testing::InMemoryAuth::default()
    });
    let app = worthfold_server::router(Arc::new(UseCases::new(
        Arc::new(InMemoryAssets::default()),
        Arc::new(InMemoryValuations::default()),
        Arc::new(InMemoryTransactions::default()),
        Arc::new(InMemoryCategories::default()),
        Arc::new(InMemoryBudget::default()),
        Arc::new(InMemoryRecurrence::default()),
        Arc::new(testing::InMemoryBank::default()),
        test_banks(),
        Arc::new(testing::StubPdfReader::default()),
        None,
        auth_ports(auth.clone()),
    )));
    (app, auth)
}

/// Une requête sans cookie de session : un navigateur qui n'est pas connecté.
fn anon(method: &str, uri: &str, form: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, HOST)
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form.to_owned()))
        .unwrap()
}

/// Le jeton posé par une réponse, tel que le navigateur le renverrait.
fn session_cookie_of(response: &Response<Body>) -> String {
    let cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .find(|c| c.starts_with("worthfold_session="))
        .expect("un cookie de session")
        .to_owned();
    cookie.split(';').next().unwrap().to_owned()
}

fn with_cookie(mut request: Request<Body>, cookie: &str) -> Request<Body> {
    request
        .headers_mut()
        .insert(header::COOKIE, cookie.parse().unwrap());
    request
}

#[tokio::test]
async fn sans_mot_de_passe_tout_mene_a_la_premiere_configuration() {
    let (app, _) = anonymous_app(false);
    for uri in ["/", "/assets", "/fragments/chart", "/login"] {
        let response = app.clone().oneshot(anon("GET", uri, "")).await.unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER, "{uri}");
        assert_eq!(header_of(&response, "location"), "/setup", "{uri}");
    }
    let page = body(app.oneshot(anon("GET", "/setup", "")).await.unwrap()).await;
    assert!(page.contains("Choisis ton mot de passe"), "{page}");
    // Pas de navigation avant d'être entré.
    assert!(!page.contains("Tableau de bord"), "{page}");
}

#[tokio::test]
async fn la_configuration_demande_le_code_puis_ouvre_la_session() {
    let (app, auth) = anonymous_app(false);
    let form = |code: &str, password: &str| {
        format!("setup_code={code}&password={password}&confirmation={password}")
    };

    // Un mauvais code, un mot de passe trop court : refusés, rien n'est enregistré.
    for (code, password) in [("FAUX", "une+phrase+de+passe"), (SETUP_CODE, "court")] {
        let response = app
            .clone()
            .oneshot(anon("POST", "/setup", &form(code, password)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert!(header_of(&response, "location").starts_with("/setup?error="));
    }
    assert_eq!(auth.session_count(), 0);

    let response = app
        .clone()
        .oneshot(anon(
            "POST",
            "/setup",
            &form(SETUP_CODE, "une+phrase+de+passe"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_of(&response, "location"), "/");
    let cookie = session_cookie_of(&response);
    let flags = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        flags.contains("HttpOnly") && flags.contains("SameSite=Lax"),
        "{flags}"
    );

    // Connecté dès la configuration ; la page de configuration n'est plus accessible.
    let home = app
        .clone()
        .oneshot(with_cookie(anon("GET", "/", ""), &cookie))
        .await
        .unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    let setup = app
        .oneshot(with_cookie(anon("GET", "/setup", ""), &cookie))
        .await
        .unwrap();
    assert_eq!(header_of(&setup, "location"), "/");
}

#[tokio::test]
async fn sans_session_on_ne_voit_que_la_connexion_et_les_fichiers_statiques() {
    let (app, _) = anonymous_app(true);
    for uri in [
        "/",
        "/assets",
        "/bank",
        "/forecast",
        "/fragments/assets",
        "/account",
    ] {
        let response = app.clone().oneshot(anon("GET", uri, "")).await.unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER, "{uri}");
        assert_eq!(header_of(&response, "location"), "/login", "{uri}");
    }
    // Une requête htmx ne peut pas suivre la redirection d'une page : elle reçoit l'ordre de changer de page.
    let mut fragment = anon("GET", "/fragments/chart", "");
    fragment
        .headers_mut()
        .insert("HX-Request", "true".parse().unwrap());
    let response = app.clone().oneshot(fragment).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(header_of(&response, "hx-redirect"), "/login");

    let login = body(
        app.clone()
            .oneshot(anon("GET", "/login", ""))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        login.contains("Se connecter") && !login.contains("Tableau de bord"),
        "{login}"
    );
    let css = app
        .oneshot(anon("GET", "/static/app.css", ""))
        .await
        .unwrap();
    assert_eq!(css.status(), StatusCode::OK);
}

#[tokio::test]
async fn sans_session_aucune_modification_n_est_acceptee() {
    let (app, _) = anonymous_app(true);
    let response = app
        .clone()
        .oneshot(anon("POST", "/assets", VALID_ASSET))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_of(&response, "location"), "/login");
    // Un cookie inventé ne vaut pas mieux.
    let forged = with_cookie(
        anon("POST", "/assets", VALID_ASSET),
        "worthfold_session=inventé",
    );
    let response = app.oneshot(forged).await.unwrap();
    assert_eq!(header_of(&response, "location"), "/login");
}

#[tokio::test]
async fn la_connexion_refuse_un_mauvais_mot_de_passe_puis_ouvre_et_ferme_la_session() {
    let (app, auth) = anonymous_app(true);
    let wrong = app
        .clone()
        .oneshot(anon("POST", "/login", "password=pas+le+bon"))
        .await
        .unwrap();
    assert!(header_of(&wrong, "location").starts_with("/login?error="));
    assert_eq!(auth.session_count(), 0);
    let page = body(
        app.clone()
            .oneshot(anon("GET", "/login?error=Mot+de+passe+incorrect.", ""))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("Mot de passe incorrect."), "{page}");

    let ok = app
        .clone()
        .oneshot(anon("POST", "/login", "password=une+phrase+de+passe"))
        .await
        .unwrap();
    assert_eq!(header_of(&ok, "location"), "/");
    let cookie = session_cookie_of(&ok);
    let page = app
        .clone()
        .oneshot(with_cookie(anon("GET", "/assets", ""), &cookie))
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);

    // Se déconnecter ferme la session côté serveur : le jeton copié ne marche plus.
    let out = app
        .clone()
        .oneshot(with_cookie(anon("POST", "/logout", ""), &cookie))
        .await
        .unwrap();
    assert_eq!(header_of(&out, "location"), "/login");
    assert_eq!(auth.session_count(), 0);
    let again = app
        .oneshot(with_cookie(anon("GET", "/assets", ""), &cookie))
        .await
        .unwrap();
    assert_eq!(header_of(&again, "location"), "/login");
}

#[tokio::test]
async fn trop_d_essais_ferment_la_connexion_meme_au_bon_mot_de_passe() {
    let (app, auth) = anonymous_app(true);
    for _ in 0..5 {
        app.clone()
            .oneshot(anon("POST", "/login", "password=faux"))
            .await
            .unwrap();
    }
    let response = app
        .oneshot(anon("POST", "/login", "password=une+phrase+de+passe"))
        .await
        .unwrap();
    assert!(
        header_of(&response, "location").contains("Trop"),
        "{}",
        header_of(&response, "location")
    );
    assert_eq!(auth.session_count(), 0);
}

#[tokio::test]
async fn changer_le_mot_de_passe_ferme_les_autres_sessions() {
    let auth = signed_in();
    // Une seconde session, ouverte ailleurs.
    use auth::AuthProvider as _;
    auth.write_session(&Session::open("ailleurs", chrono::Utc::now()))
        .await
        .unwrap();
    let app = worthfold_server::router(Arc::new(UseCases::new(
        Arc::new(InMemoryAssets::default()),
        Arc::new(InMemoryValuations::default()),
        Arc::new(InMemoryTransactions::default()),
        Arc::new(InMemoryCategories::default()),
        Arc::new(InMemoryBudget::default()),
        Arc::new(InMemoryRecurrence::default()),
        Arc::new(testing::InMemoryBank::default()),
        test_banks(),
        Arc::new(testing::StubPdfReader::default()),
        None,
        auth_ports(auth.clone()),
    )));
    let nouveau = "nouvelle+phrase+de+passe";
    let response = app
        .clone()
        .oneshot(post(
            "/account/password",
            &format!("current=une+phrase+de+passe&password={nouveau}&confirmation={nouveau}"),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(header_of(&response, "location"), "/account");
    assert_eq!(auth.session_count(), 1);
    let cookie = session_cookie_of(&response);

    // L'ancien jeton (celui des tests) et la session d'ailleurs ne servent plus ; le nouveau, oui.
    let old = app.clone().oneshot(get("/assets")).await.unwrap();
    assert_eq!(header_of(&old, "location"), "/login");
    let new = app
        .oneshot(with_cookie(anon("GET", "/assets", ""), &cookie))
        .await
        .unwrap();
    assert_eq!(new.status(), StatusCode::OK);
}

#[tokio::test]
async fn les_pages_d_entree_portent_aussi_les_en_tetes_de_securite() {
    let (app, _) = anonymous_app(true);
    let page = app.oneshot(anon("GET", "/login", "")).await.unwrap();
    assert!(header_of(&page, "content-security-policy").contains("script-src 'self'"));
    assert_eq!(header_of(&page, "cache-control"), "no-store");
}

// ── Adresse publique (derrière un proxy HTTPS) ──────────────────────────────

fn exposed_app(url: &str) -> Router {
    worthfold_server::router_exposed(
        Arc::new(UseCases::new(
            Arc::new(InMemoryAssets::default()),
            Arc::new(InMemoryValuations::default()),
            Arc::new(InMemoryTransactions::default()),
            Arc::new(InMemoryCategories::default()),
            Arc::new(InMemoryBudget::default()),
            Arc::new(InMemoryRecurrence::default()),
            Arc::new(testing::InMemoryBank::default()),
            test_banks(),
            Arc::new(testing::StubPdfReader::default()),
            None,
            auth_ports(signed_in()),
        )),
        worthfold_server::Exposure::parse(url).unwrap(),
    )
}

fn login_from(host: &str) -> Request<Body> {
    Request::post("/login")
        .header(header::HOST, host)
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("password={PASSWORD}")))
        .unwrap()
}

#[tokio::test]
async fn l_hote_public_est_accepte_et_les_cookies_sont_secure_en_https() {
    let app = exposed_app("https://worthfold.example.com");
    let response = app
        .clone()
        .oneshot(login_from("worthfold.example.com"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|c| c.to_str().unwrap().to_owned())
        .collect();
    assert!(!cookies.is_empty());
    assert!(
        cookies.iter().all(|c| c.ends_with("; Secure")),
        "{cookies:?}"
    );
    assert!(
        response
            .headers()
            .contains_key(header::STRICT_TRANSPORT_SECURITY)
    );

    // Un autre domaine pointant vers le même serveur reste refusé (DNS rebinding).
    let response = app.oneshot(login_from("piege.example")).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn en_local_les_cookies_n_ont_pas_secure_et_l_hote_public_est_refuse() {
    let response = app().oneshot(login_from(HOST)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .all(|c| !c.to_str().unwrap().contains("Secure"))
    );
    assert!(
        !response
            .headers()
            .contains_key(header::STRICT_TRANSPORT_SECURITY)
    );
    let response = app()
        .oneshot(login_from("worthfold.example.com"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ── Frein des essais, par client ────────────────────────────────────────────

fn login_with(forwarded_for: &str, password: &str) -> Request<Body> {
    Request::post("/login")
        .header(header::HOST, HOST)
        .header("x-forwarded-for", forwarded_for)
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("password={password}")))
        .unwrap()
}

#[tokio::test]
async fn un_client_qui_s_acharne_est_freine_sans_bloquer_le_proprietaire() {
    // Un Ingress de confiance devant : la dernière adresse de X-Forwarded-For est celle qu'il a vue.
    let app = worthfold_server::router_exposed(
        Arc::new(UseCases::new(
            Arc::new(InMemoryAssets::default()),
            Arc::new(InMemoryValuations::default()),
            Arc::new(InMemoryTransactions::default()),
            Arc::new(InMemoryCategories::default()),
            Arc::new(InMemoryBudget::default()),
            Arc::new(InMemoryRecurrence::default()),
            Arc::new(testing::InMemoryBank::default()),
            test_banks(),
            Arc::new(testing::StubPdfReader::default()),
            None,
            auth_ports(signed_in()),
        )),
        worthfold_server::Exposure::local().with_proxy_hops(1),
    );
    // L'attaquant change l'adresse qu'il écrit à chaque essai : seule celle de l'Ingress compte.
    for attempt in 0..5 {
        let forged = format!("10.0.0.{attempt}, 198.51.100.9");
        let response = app
            .clone()
            .oneshot(login_with(&forged, "faux"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }
    let response = app
        .clone()
        .oneshot(login_with("10.9.9.9, 198.51.100.9", PASSWORD))
        .await
        .unwrap();
    let location = response.headers()[header::LOCATION].to_str().unwrap();
    assert!(location.starts_with("/login?error="), "{location}");

    // Le propriétaire, vu d'une autre adresse, se connecte.
    let response = app
        .oneshot(login_with("203.0.113.7", PASSWORD))
        .await
        .unwrap();
    assert_eq!(response.headers()[header::LOCATION], "/");
}

// ── Banque et numéro d'un compte bancaire ───────────────────────────────────

const BANK_ACCOUNT: &str = "name=Courant&kind=bank_account&currency=EUR&bank_name=Ma+banque&account_number=fr14+2004+1010+0505+0001+3m02+606";

#[tokio::test]
async fn un_compte_montre_son_numero_masque_en_liste_et_en_entier_sur_sa_page() {
    let app = app();
    let response = app
        .clone()
        .oneshot(post("/assets", BANK_ACCOUNT, false))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let list = body(app.clone().oneshot(get("/assets")).await.unwrap()).await;
    assert!(list.contains("Ma banque · •••• 2606"), "{list}");
    assert!(
        !list.contains("FR14 2004"),
        "le numéro entier ne figure pas dans la liste"
    );

    let id = list
        .split("href=\"/assets/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_owned();
    let page = body(
        app.clone()
            .oneshot(get(&format!("/assets/{id}")))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains("Ma banque · FR14 2004 1010 0505 0001 3M02 606"),
        "{page}"
    );
    // Le formulaire de modification est prérempli : la banque du compte est présélectionnée.
    assert!(
        page.contains("<option selected>Ma banque</option>"),
        "{page}"
    );
    assert!(page.contains("value=\"FR14 2004 1010 0505 0001 3M02 606\""));
}

#[tokio::test]
async fn un_iban_mal_recopie_est_refuse_avec_le_message() {
    let form = "name=Courant&kind=bank_account&currency=EUR&bank_name=&account_number=FR1420041010050500013M02607";
    let response = app().oneshot(post("/assets", form, true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body(response).await.contains("IBAN"));
}

#[tokio::test]
async fn un_formulaire_sans_banque_ni_numero_reste_accepte() {
    let response = app()
        .oneshot(post("/assets", VALID_ASSET, false))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn le_premier_mois_d_un_rythme_se_choisit_dans_une_liste_et_non_dans_un_champ_mois() {
    // Le formulaire n'apparaît que s'il y a un compte bancaire où ranger le paiement.
    let app = app();
    app.clone()
        .oneshot(post("/assets", BANK_ACCOUNT, false))
        .await
        .unwrap();
    let page = body(app.oneshot(get("/payments")).await.unwrap()).await;
    // `<input type="month">` n'existe pas dans Firefox ni Safari de bureau : on y taperait du texte.
    assert!(!page.contains("type=\"month\""));
    let select = page
        .split("name=\"first_month\"")
        .nth(1)
        .and_then(|rest| rest.split("</select>").next())
        .unwrap();
    assert!(select.contains("décembre"));
    assert!(select.matches("<option value=\"20").count() >= 36);
}

// ── Banques et format CSV ────────────────────────────────────────────────────

/// Le formulaire d'une banque : le format le plus courant, avec ce que le test veut changer.
fn bank_form(extra: &str) -> String {
    format!(
        "encoding=utf8&delimiter=semicolon&header_line=1&date_column=Date&date_format=dmy_slash\
         &label_column=Libell%C3%A9&amount_mode=signed&amount_column=Montant&decimal_separator=comma{extra}"
    )
}

#[tokio::test]
async fn ajouter_une_banque_la_liste_puis_la_proposer_aux_comptes() {
    let app = app();
    let response = app
        .clone()
        .oneshot(post(
            "/banks",
            &bank_form("&name=Cr%C3%A9dit+Agricole"),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let page = body(app.clone().oneshot(get("/banks")).await.unwrap()).await;
    assert!(page.contains("Crédit Agricole"), "{page}");
    assert!(page.contains("aucun compte"), "{page}");
    // La banque est proposée dans le formulaire d'ajout d'un actif.
    let assets = body(app.oneshot(get("/assets")).await.unwrap()).await;
    assert!(
        assets.contains("<option>Crédit Agricole</option>"),
        "{assets}"
    );
}

#[tokio::test]
async fn un_format_invalide_ou_un_nom_deja_pris_est_refuse_avec_le_message() {
    let app = app();
    let invalid = bank_form("&name=Autre").replace("header_line=1", "header_line=0");
    let response = app
        .clone()
        .oneshot(post("/banks", &invalid, true))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body(response).await.contains("Ligne de l&#39;en-tête"));

    let duplicate = app
        .oneshot(post("/banks", &bank_form("&name=ma+BANQUE"), true))
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body(duplicate).await.contains("existe déjà"));
}

#[tokio::test]
async fn modifier_le_format_d_une_banque_change_la_lecture_des_releves() {
    let app = app();
    let id = add_asset(&app, "bank_account").await;
    let uri = format!("/assets/{id}/transactions/import");
    let statement = "Date;Libellé;Débit;Crédit\n05/11/2026;Loyer;700,00;\n";

    // Le format de « Ma banque » attend une colonne « Montant » : le relevé est refusé, avec la raison.
    let refused = app
        .clone()
        .oneshot(import(&uri, statement, ""))
        .await
        .unwrap();
    assert!(
        header_of(&refused, "location").contains("Montant"),
        "{}",
        header_of(&refused, "location")
    );

    // Une fois la banque réglée sur deux colonnes, le même relevé s'importe.
    let split = bank_form("").replace(
        "amount_mode=signed&amount_column=Montant",
        "amount_mode=debit_credit&debit_column=D%C3%A9bit&credit_column=Cr%C3%A9dit",
    );
    let updated = app
        .clone()
        .oneshot(post("/banks/Ma%20banque/update", &split, false))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::SEE_OTHER);
    let imported = app
        .clone()
        .oneshot(import(&uri, statement, ""))
        .await
        .unwrap();
    assert_eq!(imported.status(), StatusCode::SEE_OTHER);
    let list = body(
        app.oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(list.contains("Loyer"), "{list}");
}

#[tokio::test]
async fn un_compte_sans_banque_ne_peut_pas_importer_et_la_page_le_dit() {
    let app = app();
    app.clone()
        .oneshot(post(
            "/assets",
            "name=Courant&kind=bank_account&currency=EUR",
            false,
        ))
        .await
        .unwrap();
    let list = body(app.clone().oneshot(get("/assets")).await.unwrap()).await;
    let id = list
        .split("href=\"/assets/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_owned();
    let page = body(
        app.clone()
            .oneshot(get(&format!("/assets/{id}/transactions")))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("n'a pas de banque"), "{page}");

    let refused = app
        .oneshot(import(
            &format!("/assets/{id}/transactions/import"),
            STATEMENT,
            "",
        ))
        .await
        .unwrap();
    assert!(refused.status().is_redirection() || refused.status() == StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn une_banque_utilisee_ne_se_supprime_pas_une_banque_libre_oui() {
    let app = app();
    add_asset(&app, "bank_account").await;
    let refused = app
        .clone()
        .oneshot(post("/banks/Ma%20banque/delete", "", true))
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);

    app.clone()
        .oneshot(post("/banks", &bank_form("&name=Libre"), false))
        .await
        .unwrap();
    let deleted = app
        .clone()
        .oneshot(post("/banks/Libre/delete", "", false))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::SEE_OTHER);
    let page = body(app.oneshot(get("/banks")).await.unwrap()).await;
    assert!(!page.contains("Libre"), "{page}");
}

#[tokio::test]
async fn une_banque_peut_gerer_les_releves_pdf_et_un_pdf_sans_format_est_refuse() {
    let app = app();
    let pdf = "&pdf_enabled=on&pdf_date_separator=dot&pdf_debit_header=D%C3%A9bit&pdf_credit_header=Cr%C3%A9dit\
               &pdf_balance_marker=SOLDE&pdf_end_marker=TOTAL+DES+OPERATIONS";
    let response = app
        .clone()
        .oneshot(post(
            "/banks",
            &bank_form(&format!("&name=Hello+bank{pdf}")),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let page = body(app.clone().oneshot(get("/banks")).await.unwrap()).await;
    assert!(page.contains("relevé PDF géré"), "{page}");

    // Un format PDF incomplet est refusé avec le message.
    let incomplete = app
        .oneshot(post(
            "/banks",
            &bank_form(&format!("&name=Autre{}", pdf.replace("SOLDE", ""))),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(incomplete.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn la_navigation_marque_la_page_courante_et_regroupe_les_reglages() {
    let app = app();
    for (uri, label) in [
        ("/", "Accueil"),
        ("/assets", "Actifs"),
        ("/budget", "Budget"),
        ("/forecast", "Prévisions"),
        ("/settings", "Réglages"),
        ("/payments", "Réglages"),
        ("/banks", "Réglages"),
        ("/categories", "Réglages"),
        ("/account", "Réglages"),
    ] {
        let page = body(app.clone().oneshot(get(uri)).await.unwrap()).await;
        assert_eq!(page.matches("aria-current=\"page\"").count(), 1, "{uri}");
        let current = page.split("aria-current=\"page\"").nth(1).unwrap();
        let current = &current[..current.find("</a>").unwrap()];
        assert!(current.contains(label), "{uri} devrait activer {label}");
    }
}
