# worthfold

Suivi de patrimoine personnel : des actifs (comptes, livrets, titres, immobilier…) et des prêts, valorisés à des dates successives, pour connaître son patrimoine net et suivre son évolution. Application Rust (axum, askama, sqlx + PostgreSQL), design Material 3 Expressive en Tailwind CSS, mises à jour partielles avec htmx. Utilisable sans JavaScript.

Ce fichier est le sommaire de la documentation : chaque document de [`docs/`](docs/) y est référencé.

## Démarrer en local

**Avec Docker** (l'application et sa base) :

```bash
cp .env.example .env     # renseigner POSTGRES_PASSWORD : openssl rand -hex 24
docker compose up -d --build
docker compose logs app  # affiche le code de première configuration
```

**En développement** (`cargo run`), il faut une base PostgreSQL ; celle du compose convient :

```bash
docker compose -f compose.yaml -f compose.dev.yaml up -d db
cargo run -p worthfold-bootstrap
```

Prérequis du développement : Rust, l'exécutable autonome de Tailwind CSS (voir [Développement](docs/tech/development.md#environnement)) et Docker pour la base. Puis ouvrir http://localhost:3000. Le serveur n'écoute que sur la machine locale. Au premier lancement, la console affiche un code : il sert à choisir le mot de passe qui protège l'application (page `/setup`).

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
| Web | axum 0.8, un seul serveur (`127.0.0.1` par défaut), mot de passe et sessions, protégé du CSRF et du DNS rebinding, arrêt propre |
| Templates | askama 0.16 (vérifiés à la compilation), graphique en SVG généré côté serveur |
| Interactions | htmx 2.0.11, embarqué : formulaires sans rechargement, fragments qui se rechargent seuls |
| Styles | Tailwind CSS 4 (exécutable autonome, compilé au build), jetons Material 3 Expressive, police Roboto Flex embarquée |
| Persistance | PostgreSQL 17 via sqlx 0.9, migrations embarquées dans le binaire |
| Déploiement | Image Docker (binaire statique musl dans une image distroless, non-root) et `compose.yaml` avec PostgreSQL |
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
| [Déploiement](docs/tech/deployment.md) | Image Docker et compose (PostgreSQL), GitHub Actions, secrets, sauvegarde et restauration, reprise d'une ancienne base SQLite, HTTPS (`WORTHFOLD_PUBLIC_URL`), Kubernetes et Flux |
| [Tâches](docs/tech/tasks.md) | Tâches one-shot (planifiées) : le même binaire, une variable d'activation, un code de sortie ; comment en ajouter une |
| [Web](docs/tech/web.md) | Routes, htmx (fragments, formulaires, réponses), erreurs, templates, mise en forme des montants, graphique |
| [Sécurité](docs/tech/security.md) | Modèle de menace, authentification (mot de passe, sessions), garde des requêtes, échappement, secrets et connexion bancaire, écarts connus à corriger |

### Design

| Document | Contenu |
|---|---|
| [Interface (fondations)](docs/design/foundations.md) | Interface utilisateur, design system Material 3 Expressive : couleurs et contrastes, typographie, formes, mouvement, états, composants, accessibilité, audit |
| [Expérience utilisateur](docs/design/ux.md) | Pour qui et pour quoi, parcours principaux, douze règles de comportement (retours, prévention des erreurs, messages, fraîcheur et origine des données…), audits |
| [Écrans](docs/design/screens.md) | Application aux écrans : navigation, tableau de bord, Actifs, Opérations, Catégories, Budget, Paiements, Prévisions, Banque, graphique |

### Métier

| Document | Contenu |
|---|---|
| [Actifs et valorisations](docs/specs/assets-and-valuations.md) | Types d'actifs, devises, historique, prêts, règles de modification et de suppression |
| [Transactions et import de relevés](docs/specs/transactions.md) | Opérations d'un compte bancaire, import CSV selon la banque du compte (doublons, refus), solde du compte |
| [Catégories et règles](docs/specs/categories.md) | Catégories et leur sens, règles d'affectation par libellé, classement à l'import, à la demande et à la main |
| [Budget et épargne](docs/specs/budget.md) | Synthèse du mois, enveloppes par catégorie, matelas de sécurité évalué, versement d'épargne suggéré |
| [Paiements prévus](docs/specs/payments.md) | Rythmes, montants par paliers et révisions, exceptions, rapprochement avec les relevés, suggestions détectées |
| [Banques et formats de relevé](docs/specs/banks.md) | Une banque dit comment décoder ses exports CSV (encodage, séparateur, colonnes, dates, montants) et ses relevés PDF (colonnes par position, soldes vérifiés) ; rattachement des comptes |
| [Connexion bancaire](docs/specs/bank.md) | EnableBanking : consentement de 90 jours, rattachement des comptes, lecture des opérations et du solde, renouvellement, tâche planifiée |
| [Prévisions de trésorerie](docs/specs/forecast.md) | Solde et patrimoine projetés mois après mois, trois scénarios, alertes de découvert et de matelas |
| [Patrimoine net et évolution](docs/specs/net-worth.md) | Calcul du total, de l'historique, et limites actuelles |

## Licence

[MIT](LICENSE). Les fichiers tiers embarqués gardent leur licence : htmx (0BSD), Roboto Flex (SIL OFL 1.1, `crates/server/assets/fonts/OFL.txt`).
