//! La page Catégories (`/categories`) : les catégories, les règles qui classent les libellés,
//! et le reclassement des opérations sans catégorie.
//!
//! Les formulaires sont envoyés sans htmx (ils marchent sans JavaScript) : un succès redirige vers la page
//! avec une confirmation (message flash), un refus la rouvre avec le message en haut. Les suppressions
//! demandent confirmation avec htmx (`hx-confirm`), comme ailleurs.

use application::{
    UseCaseError, add_category, add_category_rule, add_default_categories, apply_category_rules,
    delete_category, delete_category_rule, read_categories,
};
use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use category::CategoryKind;
use serde::Deserialize;

use super::AppState;
use super::reply::{Reply, render, technical_page};
use super::views::{ErrorParams, with_flash_consumed};
use crate::flash;

const PAGE: &str = "/categories";

struct CategoryView {
    id: String,
    name: String,
    kind: &'static str,
    /// Les motifs qui classent dans cette catégorie.
    rules: Vec<RuleView>,
    delete_confirm: String,
}

struct RuleView {
    id: String,
    pattern: String,
    delete_confirm: String,
}

#[derive(Template)]
#[template(path = "categories_page.html")]
struct CategoriesPage {
    section: &'static str,
    categories: Vec<CategoryView>,
    kinds: Vec<(&'static str, &'static str)>,
    error: Option<String>,
    flash: Option<String>,
}

// ── GET /categories ──────────────────────────────────────────────────────────

pub async fn get_categories_page(
    State(app): AppState,
    Query(params): Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    let output = match app.read_categories.handle(read_categories::Query).await {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    let categories = output
        .categories
        .into_iter()
        .map(|category| {
            let rules: Vec<RuleView> = output
                .rules
                .iter()
                .filter(|rule| rule.category_id == category.id)
                .map(|rule| RuleView {
                    id: rule.id.to_string(),
                    pattern: rule.pattern.clone(),
                    delete_confirm: format!(
                        "Supprimer la règle « {} » ? Les opérations déjà classées gardent leur catégorie.",
                        rule.pattern
                    ),
                })
                .collect();
            CategoryView {
                id: category.id.to_string(),
                kind: category.kind.label(),
                delete_confirm: format!(
                    "Supprimer « {} » et ses règles ? Les opérations classées ici redeviendront sans catégorie.",
                    category.name
                ),
                name: category.name,
                rules,
            }
        })
        .collect();
    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(CategoriesPage {
        section: "categories",
        categories,
        kinds: CategoryKind::ALL
            .iter()
            .map(|k| (k.code(), k.label()))
            .collect(),
        error: params.error,
        flash,
    });
    with_flash_consumed(page, had_flash)
}

// ── POST /categories ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CategoryForm {
    name: String,
    kind: String,
}

pub async fn post_category(
    State(app): AppState,
    headers: HeaderMap,
    Form(form): Form<CategoryForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    let Some(kind) = CategoryKind::from_code(&form.kind) else {
        return reply.refusal("Le sens de la catégorie est invalide.");
    };
    match app
        .add_category
        .handle(add_category::Command {
            name: form.name,
            kind,
        })
        .await
    {
        Ok(output) => reply.redirect(PAGE, &format!("« {} » ajoutée.", output.category.name)),
        Err(e) => reply.error(e),
    }
}

// ── POST /categories/defaults ────────────────────────────────────────────────

pub async fn post_default_categories(State(app): AppState, headers: HeaderMap) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .add_default_categories
        .handle(add_default_categories::Command)
        .await
    {
        Ok(output) => reply.redirect(
            PAGE,
            &match output.added {
                0 => "Les catégories usuelles existent déjà.".to_owned(),
                1 => "1 catégorie usuelle ajoutée.".to_owned(),
                n => format!("{n} catégories usuelles ajoutées."),
            },
        ),
        Err(e) => reply.error(e),
    }
}

// ── POST /categories/{id}/delete ─────────────────────────────────────────────

pub async fn post_category_delete(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .delete_category
        .handle(delete_category::Command {
            category_id: id.into(),
        })
        .await
    {
        Ok(output) => reply.redirect(PAGE, &format!("« {} » supprimée.", output.category.name)),
        Err(e) => reply.error(e),
    }
}

// ── POST /categories/rules ───────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RuleForm {
    category_id: String,
    pattern: String,
}

pub async fn post_rule(
    State(app): AppState,
    headers: HeaderMap,
    Form(form): Form<RuleForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .add_category_rule
        .handle(add_category_rule::Command {
            category_id: form.category_id.into(),
            pattern: form.pattern,
        })
        .await
    {
        Ok(output) => reply.redirect(
            PAGE,
            &format!(
                "Règle « {} » ajoutée : « Classer les opérations » l'applique à celles déjà importées.",
                output.rule.pattern
            ),
        ),
        Err(e) => reply.error(e),
    }
}

// ── POST /categories/rules/{id}/delete ───────────────────────────────────────

pub async fn post_rule_delete(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .delete_category_rule
        .handle(delete_category_rule::Command { rule_id: id.into() })
        .await
    {
        Ok(output) => reply.redirect(
            PAGE,
            &format!("Règle « {} » supprimée.", output.rule.pattern),
        ),
        Err(e) => reply.error(e),
    }
}

// ── POST /categories/apply ───────────────────────────────────────────────────

pub async fn post_apply_rules(State(app): AppState, headers: HeaderMap) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .apply_category_rules
        .handle(apply_category_rules::Command)
        .await
    {
        Ok(output) => reply.redirect(
            PAGE,
            &match output.classified {
                0 => "Aucune opération sans catégorie ne correspond à une règle.".to_owned(),
                1 => "1 opération classée.".to_owned(),
                n => format!("{n} opérations classées."),
            },
        ),
        Err(UseCaseError::Refusal(message)) => reply.refusal(&message),
        Err(e) => reply.error(e),
    }
}
