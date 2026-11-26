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
| `cargo run -p worthfold-bootstrap` | Lance l'application sur http://localhost:3000 (binaire `worthfold`) ; demande une base PostgreSQL (voir [Base de données](#base-de-données)) |
| `docker compose up -d --build` | Lance l'application et sa base dans des conteneurs (voir [Déploiement](deployment.md)) |
| `cargo check` | Vérifie sans compiler entièrement : le plus rapide |
| `bacon clippy` | Relance clippy à chaque sauvegarde, dans un terminal à côté |
| `cargo nextest run` | Tous les tests (`cargo test` fonctionne aussi) |
| `cargo nextest run -p worthfold-application` | Les tests d'une seule crate |
| `cargo clippy --workspace --all-targets` | Le linter sur tout le workspace, tests compris |
| `cargo fmt` | Formate le code |
| `cargo audit` | Cherche des failles connues dans les dépendances (`cargo install cargo-audit --locked` une fois) ; à lancer avant une mise à jour de `Cargo.lock` et de temps en temps |

Le code doit passer clippy sans avertissement.

## Configuration

Variables d'environnement, lues une seule fois au démarrage par la configuration du point d'entrée (`crates/bootstrap/src/config.rs`). Un fichier `.env` à la racine est chargé s'il existe (ignoré par git).

| Variable | Défaut | Rôle |
|---|---|---|
| `WORTHFOLD_DATABASE_URL` | **obligatoire** | Adresse de la base PostgreSQL : `postgres://utilisateur:mot_de_passe@hôte:5432/base`. Elle porte le mot de passe : jamais dans un journal ni dans une image |
| `WORTHFOLD_PORT` | `3000` | Port d'écoute |
| `WORTHFOLD_BIND_ADDRESS` | `127.0.0.1` | Interface d'écoute. Laisser la machine locale ; seul un conteneur écoute sur `0.0.0.0` (l'image le fait, et le compose ne publie le port que sur `127.0.0.1`) |
| `WORTHFOLD_TEST_DATABASE_URL` | absente | Un serveur PostgreSQL où créer des bases jetables : lance les tests de persistance (sinon ils sont ignorés). Voir [Tests](#tests) |
| `RUST_LOG` | `info` | Niveau des journaux (`warn`, `debug`…) |
| `WORTHFOLD_PUBLIC_URL` | absente | Adresse publique derrière un proxy HTTPS (`https://worthfold.example.com`, sans chemin) : hôte accepté en plus de la machine locale, cookies `Secure`, HSTS, adresse de retour de la banque. Voir [Déploiement](deployment.md#exposer-hors-de-la-machine) |
| `WORTHFOLD_PROXY_HOPS` | `0` | Nombre de proxys de confiance devant l'application (1 : un Ingress). Sert à retrouver l'adresse du client dans `X-Forwarded-For` pour le frein des essais de mot de passe. 0 : l'adresse de la connexion, l'en-tête est ignoré |
| `WORTHFOLD_ENABLEBANKING_APP_ID`, `WORTHFOLD_ENABLEBANKING_KEY_PATH`, `WORTHFOLD_BANK_REDIRECT_URL` | absentes | Connexion bancaire, voir [Connexion bancaire](../specs/bank.md) |
| `WORTHFOLD_BANK_SYNC_TASK` | absente | Si définie, lance la tâche de lecture des comptes au lieu du serveur, voir [Tâches](tasks.md) |
| `WORTHFOLD_RESET_PASSWORD_TASK` | absente | Si définie, retire le mot de passe et les sessions au lieu de lancer le serveur (mot de passe oublié), voir [Tâches](tasks.md). Ne jamais la mettre dans `.env` |

**Premier lancement** : aucun mot de passe n'existe. La console affiche une ligne « Première configuration : ouvre la page /setup de worthfold et saisis le code … » ; ce code, valable pour cette exécution, sert à choisir le mot de passe (voir [Sécurité](security.md#authentification)).

Une valeur vide compte comme absente. Une valeur invalide empêche le démarrage, avec un message clair.

## Styles

- Source : `crates/server/styles/app.css` (jetons et composants, voir [Fondations](../design/foundations.md)).
- `crates/server/build.rs` lance Tailwind à chaque build où `styles/` ou `templates/` change, et écrit la feuille compilée et minifiée dans `$OUT_DIR/app.css`, embarquée dans le binaire et servie par `/static/app.css`. Il n'y a pas de fichier CSS généré à versionner.
- Tailwind ne génère que les classes présentes dans les templates (`@source "../templates"`) : une classe construite dynamiquement en Rust ne serait pas générée.
- Exécutable cherché dans `$TAILWINDCSS`, puis `~/.local/bin/tailwindcss`, puis le `PATH`. S'il manque, le build échoue avec un message qui renvoie ici.

## Base de données

PostgreSQL 17. Pour le développement, le plus simple est celui du compose, publié sur la machine :

```bash
cp .env.example .env                       # puis renseigner POSTGRES_PASSWORD (openssl rand -hex 24)
docker compose -f compose.yaml -f compose.dev.yaml up -d db
```

La base est alors sur `127.0.0.1:5433` (voir [`compose.dev.yaml`](../../compose.dev.yaml)) ; `WORTHFOLD_DATABASE_URL` de `.env` y pointe. Toutes les données sont dans cette base : **la sauvegarder, c'est sauvegarder toute l'application** ([Déploiement](deployment.md#sauvegarde-et-restauration)).

- Les migrations sont dans `crates/infrastructure/migrations/` (une seule pour l'instant : `0001_schema.sql`). Elles sont embarquées dans le binaire (`sqlx::migrate!`) et appliquées à chaque démarrage. Une migration déjà appliquée ne se modifie jamais (somme de contrôle) : on en ajoute une nouvelle (`0002_….sql`).
- `build.rs` force la recompilation de `infrastructure` quand une migration change.
- **Types** : les montants sont des `NUMERIC` (décimal exact, jamais un flottant), les jours des `DATE`, les instants des `TIMESTAMPTZ`, les booléens des `BOOLEAN`.
- Les clés étrangères sont vérifiées : une valorisation ne peut pas exister sans son actif.
- Les identifiants sont des UUID v7 en `TEXT`, générés par l'application ; triés, ils suivent l'ordre de saisie.
- sqlx refuse une requête construite dynamiquement (`format!`) : les requêtes s'écrivent en entier, les valeurs passent en paramètres `$1`, `$2`… Seule exception : le nom d'une base jetable des tests, généré par le test lui-même (`AssertSqlSafe`).
- **Venir de SQLite** : l'application a d'abord utilisé SQLite. `scripts/sqlite-to-postgres.py` reprend un ancien `worthfold.db` dans PostgreSQL (voir [Déploiement](deployment.md#reprendre-une-ancienne-base-sqlite)).

## Tests

- **Règles du domaine et services** : des tests unitaires dans le même fichier (`#[cfg(test)]`), sans doublure.
- **Cas d'usage** : sur les providers en mémoire de la crate `testing` (`InMemoryAssets`, `InMemoryValuations`). On vérifie un comportement (ce qui est écrit, ce qui est refusé), pas des appels.
- **Persistance** : sur un vrai PostgreSQL. Chaque test crée une base jetable (`worthfold_test_…`) sur le serveur de `WORTHFOLD_TEST_DATABASE_URL`, y applique les migrations, puis la supprime. Sans cette variable, ces tests sont **ignorés** (ils passent sans rien vérifier, et le disent dans leur sortie) : pour les lancer, `docker compose -f compose.yaml -f compose.dev.yaml up -d db` puis `WORTHFOLD_TEST_DATABASE_URL=postgres://worthfold:<mot de passe>@localhost:5433/postgres cargo nextest run`. L'utilisateur doit pouvoir créer des bases (c'est le cas de l'utilisateur du compose).
- **HTTP** (`crates/server/tests/http.rs`) : le routeur complet sur les providers en mémoire (`UseCases::new`), appelé avec `tower::ServiceExt::oneshot`, sans ouvrir de port. On vérifie la traduction en réponses : avec et sans htmx, refus (422 ou 303 avec `?error=`), succès (`HX-Trigger` ou message flash), pages introuvables.
- **Présentation** : la mise en forme des montants et le calcul des graduations du graphique ont leurs propres tests.
- Les noms de tests décrivent le comportement, en français : `refuse_un_pret_saisi_en_negatif`.
