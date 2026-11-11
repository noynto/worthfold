# Développement

## Environnement

Rust s'installe dans l'utilisateur avec rustup ; seul le compilateur C (éditeur de liens) demande les droits administrateur.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
rustup component add rust-analyzer rust-src
sudo apt install build-essential pkg-config
cargo install --locked bacon cargo-nextest
```

Tailwind CSS : l'exécutable autonome (un seul binaire, sans Node ni npm), dans `~/.local/bin`. Vérifier l'empreinte avec le fichier `sha256sums.txt` de la même version :

```bash
V=v4.3.5
curl -sSfLO https://github.com/tailwindlabs/tailwindcss/releases/download/$V/tailwindcss-linux-x64
curl -sSfLO https://github.com/tailwindlabs/tailwindcss/releases/download/$V/sha256sums.txt
grep 'tailwindcss-linux-x64$' sha256sums.txt | sed 's#\./##' | sha256sum -c -
install -m 755 tailwindcss-linux-x64 ~/.local/bin/tailwindcss
```

Mise à jour : `rustup update`.

Éditeur : Zed gère Rust nativement. Sa configuration (`~/.config/zed/settings.json`) utilise le `rust-analyzer` de rustup et lance clippy à chaque sauvegarde.

## Commandes

| Commande | Rôle |
|---|---|
| `cargo run -p worthfold-bootstrap` | Lance l'application sur http://localhost:3000 (binaire `worthfold`) |
| `cargo check` | Vérifie sans compiler entièrement : le plus rapide |
| `bacon clippy` | Relance clippy à chaque sauvegarde, dans un terminal à côté |
| `cargo nextest run` | Tous les tests (`cargo test` fonctionne aussi) |
| `cargo nextest run -p worthfold-application` | Les tests d'une seule crate |
| `cargo clippy --workspace --all-targets` | Le linter sur tout le workspace, tests compris |
| `cargo fmt` | Formate le code |

Le code doit passer clippy sans avertissement.

## Configuration

Variables d'environnement, lues une seule fois au démarrage par la configuration du point d'entrée (`crates/bootstrap/src/config.rs`). Un fichier `.env` à la racine est chargé s'il existe (ignoré par git).

| Variable | Défaut | Rôle |
|---|---|---|
| `WORTHFOLD_DATABASE_URL` | `sqlite://worthfold.db` | Fichier de la base SQLite, créé s'il n'existe pas |
| `WORTHFOLD_PORT` | `3000` | Port d'écoute, toujours sur `127.0.0.1` |

Une valeur vide compte comme absente. Une valeur invalide empêche le démarrage, avec un message clair.

## Styles

- Source : `crates/server/styles/app.css` (jetons et composants, voir [Fondations](../design/foundations.md)).
- `crates/server/build.rs` lance Tailwind à chaque build où `styles/` ou `templates/` change, et écrit la feuille compilée et minifiée dans `$OUT_DIR/app.css`, embarquée dans le binaire et servie par `/static/app.css`. Il n'y a pas de fichier CSS généré à versionner.
- Tailwind ne génère que les classes présentes dans les templates (`@source "../templates"`) : une classe construite dynamiquement en Rust ne serait pas générée.
- Exécutable cherché dans `$TAILWINDCSS`, puis `~/.local/bin/tailwindcss`, puis le `PATH`. S'il manque, le build échoue avec un message qui renvoie ici.

## Base de données

- Toutes les données sont dans un seul fichier SQLite (`worthfold.db` par défaut, ignoré par git). **Le sauvegarder, c'est sauvegarder toute l'application.**
- Les migrations sont dans `crates/infrastructure/migrations/`, numérotées (`0001_init.sql`, `0002_valuation_ids.sql`…). Elles sont embarquées dans le binaire (`sqlx::migrate!`) et appliquées à chaque démarrage. Une migration déjà appliquée ne se modifie jamais : on en ajoute une nouvelle.
- `build.rs` force la recompilation de `infrastructure` quand une migration change.
- Les montants sont stockés en `TEXT` (décimal exact) et les dates en `TEXT` ISO 8601 (`AAAA-MM-JJ`), jamais en `REAL`.
- Les clés étrangères sont activées : une valorisation ne peut pas exister sans son actif.
- Les identifiants sont des UUID v7 en `TEXT`, générés par l'application ; triés, ils suivent l'ordre de saisie. La migration `0002` a converti les anciens identifiants entiers des valorisations en UUID au temps zéro, numérotés (`00000000-0000-7000-8000-000000000001`…), pour garder leur ordre ; un test le vérifie sur une base d'avant la migration.
- sqlx refuse une requête construite dynamiquement (`format!`) : les requêtes s'écrivent en entier, les valeurs passent en paramètres `?`.

## Tests

- **Règles du domaine et services** : des tests unitaires dans le même fichier (`#[cfg(test)]`), sans doublure.
- **Cas d'usage** : sur les providers en mémoire de la crate `testing` (`InMemoryAssets`, `InMemoryValuations`). On vérifie un comportement (ce qui est écrit, ce qui est refusé), pas des appels.
- **Persistance** : sur une vraie base SQLite, dans un fichier temporaire propre à chaque test. Ne pas utiliser `sqlite::memory:` avec un pool : chaque connexion y verrait une base différente et vide.
- **HTTP** (`crates/server/tests/http.rs`) : le routeur complet sur les providers en mémoire (`UseCases::new`), appelé avec `tower::ServiceExt::oneshot`, sans ouvrir de port. On vérifie la traduction en réponses : avec et sans htmx, refus (422 ou 303 avec `?error=`), succès (`HX-Trigger` ou message flash), pages introuvables.
- **Présentation** : la mise en forme des montants et le calcul des graduations du graphique ont leurs propres tests.
- Les noms de tests décrivent le comportement, en français : `refuse_un_pret_saisi_en_negatif`.
