# worthfold

Suivi de patrimoine personnel : des actifs (comptes, livrets, titres, immobilier…) et des prêts, valorisés à des dates successives, pour connaître son patrimoine net et suivre son évolution. Application Rust (axum, askama, sqlx + SQLite), design Material 3 Expressive en Tailwind CSS, mises à jour partielles avec htmx. Utilisable sans JavaScript.

Ce fichier est le sommaire de la documentation : chaque document de [`docs/`](docs/) y est référencé.

## Démarrer en local

```bash
cargo run -p worthfold-bootstrap
```

Prérequis : Rust et l'exécutable autonome de Tailwind CSS (voir [Développement](docs/tech/development.md#environnement)). Puis ouvrir http://localhost:3000. Le serveur n'écoute que sur la machine locale ; la base `worthfold.db` est créée au premier lancement, à la racine du projet.

## Commandes

```bash
cargo run -p worthfold-bootstrap # lance l'application (binaire `worthfold`)
bacon clippy                 # vérification continue pendant le développement
cargo nextest run            # tous les tests (ou : cargo test)
cargo clippy --workspace --all-targets
cargo fmt
```

Détails dans [Développement](docs/tech/development.md).

## Pile technique

| Couche | Technologie |
|---|---|
| Web | axum 0.8, un seul serveur sur `127.0.0.1`, protégé du CSRF et du DNS rebinding, arrêt propre |
| Templates | askama 0.16 (vérifiés à la compilation), graphique en SVG généré côté serveur |
| Interactions | htmx 2.0.11, embarqué : formulaires sans rechargement, fragments qui se rechargent seuls |
| Styles | Tailwind CSS 4 (exécutable autonome, compilé au build), jetons Material 3 Expressive, police Roboto Flex embarquée |
| Persistance | SQLite via sqlx 0.9, migrations embarquées dans le binaire |
| Montants | `rust_decimal` : décimaux exacts, jamais de flottant |
| Tests | tests intégrés à Rust, `cargo-nextest`, providers en mémoire, tests HTTP sur le routeur complet |
| Outillage | rustup (stable), clippy, rustfmt, rust-analyzer, bacon |

## Les règles en bref

1. **Seuls les cas d'usage parlent aux providers.** Un handler appelle un seul cas d'usage ; une page qui a besoin d'un autre cas d'usage le charge par un fragment htmx ; un service ne fait que calculer.
2. **Un cas d'usage rend un `Output` ou une `UseCaseError`** : `Refusal` (message en français, montré tel quel) ou `Technical` (journalisée, réponse 500).
3. **Une règle qui croise plusieurs domaines est une fonction pure** dans `service`, sans entrée-sortie.
4. **L'argent est un `Money`** : un montant décimal et sa devise. Additionner deux devises différentes est une erreur.
5. **L'historique ne s'écrase jamais** : une valorisation s'ajoute, elle ne remplace pas la précédente.
6. **Pas de HTML ni de JavaScript construits par concaténation** : tout passe par un template, qui échappe les valeurs.
7. **Jetons du design system uniquement** : couleurs par rôle M3, tailles, arrondis et mouvements déclarés dans `app.css` ; jamais de valeur arbitraire.

## Documentation

### Technique

| Document | Contenu |
|---|---|
| [Architecture](docs/tech/architecture.md) | Couches, crates et sens des dépendances, cas d'usage, providers, services, ajouter une fonctionnalité |
| [Développement](docs/tech/development.md) | Installer l'environnement (Rust, Tailwind), commandes, configuration, compilation des styles, base de données et migrations, tests |
| [Tâches](docs/tech/tasks.md) | Tâches one-shot (planifiées) : le même binaire, une variable d'activation, un code de sortie ; comment en ajouter une |
| [Web](docs/tech/web.md) | Routes, htmx (fragments, formulaires, réponses), erreurs, templates, mise en forme des montants, graphique, sécurité |

### Design

| Document | Contenu |
|---|---|
| [Fondations](docs/design/foundations.md) | Material 3 Expressive : couleurs et contrastes, typographie, formes, mouvement, états, composants, accessibilité |
| [Règles d'UX](docs/design/ux.md) | Dix règles de comportement (retours, prévention des erreurs, messages, fraîcheur des données…) et l'audit de l'interface |
| [Écrans](docs/design/screens.md) | Application aux écrans : navigation, tableau de bord, page Actifs, page d'un actif (valorisation, historique), graphique |

### Métier

| Document | Contenu |
|---|---|
| [Actifs et valorisations](docs/specs/assets-and-valuations.md) | Types d'actifs, devises, historique, prêts, règles de modification et de suppression |
| [Patrimoine net et évolution](docs/specs/net-worth.md) | Calcul du total, de l'historique, et limites actuelles |

## Licence

[MIT](LICENSE). Les fichiers tiers embarqués gardent leur licence : htmx (0BSD), Roboto Flex (SIL OFL 1.1, `crates/server/assets/fonts/OFL.txt`).
