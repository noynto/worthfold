# Architecture

worthfold est un workspace Cargo découpé en crates, une par couche ou par domaine. Il n'y a pas de framework d'injection : `bootstrap`, le point d'entrée, lit la configuration, construit toutes les dépendances à la main, puis lance soit une tâche, soit le serveur web. L'organisation reprend celle d'EOSA : domaine, services, cas d'usage, infrastructure, et un `Bootstrap` qui choisit quoi lancer.

## Crates

Chaque package porte le préfixe `worthfold-` (`worthfold-asset`, `worthfold-server`…), pour éviter toute confusion avec une crate publique. Le dossier garde le nom court (`crates/asset`), et les crates dépendantes renomment la dépendance (`asset = { package = "worthfold-asset", path = "../asset" }`) : le code écrit `use asset::…`. Le seul binaire, celui de `worthfold-bootstrap`, s'appelle `worthfold`.

```
crates/
├── shared/          identifiants et valeurs partagées : AssetId, Money, Currency
├── asset/           domaine des actifs : Asset, AssetKind, trait AssetProvider
├── valuation/       domaine des valorisations : Valuation, trait ValuationProvider
├── service/         règles pures qui croisent plusieurs domaines
├── application/     cas d'usage, un module par action, et `UseCases` qui les réunit
├── testing/         providers en mémoire, pour les tests (dev-dependency uniquement)
├── infrastructure/  implémentations des providers (sqlx + SQLite) et migrations
├── server/          adaptateur web (bibliothèque) : axum, handlers, templates, mise en forme, graphique
└── bootstrap/       binaire `worthfold` : configuration, assemblage, puis serveur ou tâche
```

## Sens des dépendances

```
bootstrap (binaire) ──▶ server ─────────┐
        │                               ▼
        ├──────────────────────────▶ application ──▶ service ──▶ asset, valuation, shared
        │
        └──▶ infrastructure ──▶ asset, valuation, shared
```

- `asset`, `valuation` et `shared` ne dépendent d'aucune bibliothèque technique : ni sqlx, ni axum.
- `application` ne connaît que le domaine et les services. Le compilateur l'empêche d'utiliser sqlx, même par accident.
- **Seul `bootstrap` voit l'infrastructure.** Le serveur (et demain les tâches) ne reçoit que `application::UseCases` : il ignore SQLite comme la façon dont l'application est assemblée.
- `server` est une bibliothèque : elle ne peut pas dépendre de `bootstrap`, qui dépend d'elle. C'est pourquoi la structure qui réunit les cas d'usage (`UseCases`) vit dans `application`, la seule crate qui les connaît tous.
- `testing` n'est ajoutée qu'en `[dev-dependencies]` : elle n'entre jamais dans un binaire.

## Point d'entrée (`bootstrap`)

Le `main` du binaire `worthfold` :

1. charge le fichier `.env` s'il existe et initialise les journaux ;
2. lit la configuration (`config.rs`) : une valeur invalide arrête le programme avec un message clair et le code 1 ;
3. ouvre la base, applique les migrations et branche les adaptateurs SQLite sur les cas d'usage (`assembly.rs`, via `UseCases::new`) ;
4. lance **soit une tâche** (si sa variable d'activation est définie : elle s'exécute, puis le programme s'arrête avec le code 0 ou 1), **soit le serveur web** (`server::serve`), qui tourne jusqu'à Ctrl+C ou SIGTERM puis s'arrête proprement : il n'accepte plus de connexion et laisse finir les requêtes en cours. Voir [Tâches](tasks.md).

Un seul binaire, donc une seule image : le serveur et les tâches planifiées tournent avec la même version.

## Domaine

- Un objet métier est une struct aux champs publics (`Asset`, `Valuation`) ; les petites valeurs sont des types dédiés (`AssetId`, `Money`).
- Une règle qui ne concerne qu'un domaine vit avec lui : `Asset::validate_name`, `AssetKind::is_liability`.
- Chaque domaine expose un trait `XProvider` limité à la lecture et à l'écriture (`read_all`, `read`, `write`, `delete`…). **Aucune règle métier dans un provider** : la suppression de l'historique d'un actif, par exemple, est décidée par le cas d'usage `DeleteAsset`.
- Les providers sont `Send + Sync` et utilisés derrière `Arc<dyn XProvider>`, avec `async-trait`.

## Cas d'usage (`application`)

Un module par action, nommé par elle (`add_asset`, `record_valuation`) :

```rust
pub struct RecordValuation { pub asset_provider: Arc<dyn AssetProvider>, /* … */ }
pub struct Command { /* … */ }   // ou Query pour une lecture
pub struct Output { /* … */ }    // absent si rien à rendre

impl RecordValuation {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> { … }
}
```

- Il vérifie la commande, lit avec ses providers, applique les règles (méthodes du domaine, services), puis écrit.
- **Il n'appelle jamais un autre cas d'usage.** Un peu de répétition entre cas d'usage est acceptée : chacun se lit seul, de haut en bas.
- L'`Output` porte tout ce dont l'appelant a besoin (`ReadAsset` rend l'actif et son historique trié, du plus récent au plus ancien).
- `UseCaseError::Refusal(String)` : un refus de règle métier, avec un message en français prêt à être affiché. `UseCaseError::Technical(anyhow::Error)` : une panne, convertie automatiquement par `?`.

Cas d'usage actuels : `AddAsset`, `ReadAsset`, `UpdateAsset`, `DeleteAsset`, `RecordValuation`, `DeleteValuation`, `ReadNetWorth`, `ReadNetWorthHistory`.

## Services (`service`)

Des fonctions pures qui reçoivent des objets déjà lus et rendent une valeur : `latest_valuation`, `net_worth`, `breakdown` (avoirs et dettes), `net_worth_history`. Elles ne lisent ni la base, ni l'horloge (la date est un paramètre), et se testent sans doublure.

## La date du jour

Rien en dessous de `server` ne lit l'horloge : la date du jour est une **valeur passée explicitement**, dans la `Command` ou la `Query` (`RecordValuation::Command::today`, `ReadNetWorth::Query::at`, `ReadNetWorthHistory::Query::until`), puis aux méthodes du domaine et aux services. Pas de trait `Clock` à injecter : une fonction qui reçoit tout ce dont elle dépend se lit et se teste sans doublure (un test passe une date fixe).

Côté web, `handlers::today()` est la seule lecture de l'horloge : chaque handler l'appelle une fois et passe la même date au cas d'usage et aux vues.

## Infrastructure

`PersistedAssets` et `PersistedValuations` implémentent les providers avec sqlx. Les requêtes sont écrites en SQL, avec des paramètres `?` : aucune valeur n'est concaténée dans une requête. Voir [Développement](development.md#base-de-données).

## Handlers (`server`)

Un handler lit la requête vers la `Command`, appelle **un seul** cas d'usage, puis traduit le résultat en réponse : une page, un fragment, ou la réponse d'un formulaire (htmx ou non, voir [Web](web.md#formulaires)). Il ne contient aucune règle métier et ne parle à aucun provider.

Les handlers sont rangés par page dans `crates/server/src/handlers/` : `dashboard.rs`, `assets_page.rs` et `asset_page.rs` (chacun avec ses fragments et ses formulaires), `valuations.rs` (les formulaires de valorisation), `views.rs` (ce que plusieurs pages partagent) et `reply.rs` (rendu, réponses de formulaire, pannes, page introuvable).

### Pages composées avec htmx

Quand une page affiche des informations venant de cas d'usage différents, le **handler de la page ne sert que l'essentiel** (un cas d'usage), et le navigateur va chercher le reste par des fragments, un handler et un cas d'usage par fragment.

- L'essentiel : ce qu'il faut pour utiliser la page sans JavaScript. Sur le tableau de bord : le total, les avoirs et dettes et les actifs à mettre à jour ; sur la page Actifs : la liste (`ReadNetWorth` dans les deux cas).
- Le complément : le graphique (`ReadNetWorthHistory`), chargé par `hx-get="/fragments/chart"` avec un squelette à sa forme, et servi en SVG autonome dans un `<noscript>`.
- Après une modification, la page n'est pas rechargée : le formulaire émet `worth-changed`, et chaque fragment concerné se redemande lui-même. Le handler du formulaire n'appelle toujours qu'un cas d'usage.
- Un handler qui aurait besoin d'un deuxième cas d'usage ne l'appelle pas : c'est la page qui demande ce complément par un fragment.

## Limites connues

- **Pas de transaction entre providers.** `DeleteAsset` supprime les valorisations puis l'actif en deux écritures : si la seconde échoue, l'actif reste sans historique. Accepté pour une application locale à un seul utilisateur. Évolution prévue : une unité de travail transactionnelle, que le cas d'usage ouvre et valide, plutôt qu'un `ON DELETE CASCADE` qui cacherait la suppression dans le schéma.

## Ajouter une fonctionnalité

1. Si besoin, la donnée et la règle dans le domaine (`asset`, `valuation`), et la méthode dans le provider.
2. L'implémentation du provider dans `infrastructure` (et une migration si le schéma change), puis dans `testing`.
3. Le cas d'usage dans `application`, avec ses tests sur les providers en mémoire.
4. Son champ dans `application::UseCases` et son branchement dans `UseCases::new` (utilisé par `bootstrap` comme par les tests HTTP).
5. Le handler, la route et le template dans `server` ; un formulaire suit le modèle de [Web](web.md#formulaires) (cible d'erreur, `hx-post`, réponse via `Reply`).
6. La mise à jour de la documentation concernée et, si un document est ajouté, de l'index du README.
