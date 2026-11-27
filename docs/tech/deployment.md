# Déploiement

worthfold se déploie avec Docker : une image qui ne contient que le binaire, et un `compose.yaml` qui la lance avec PostgreSQL. Ce document décrit ces fichiers, comment les utiliser, et ce qu'ils garantissent.

## Les fichiers

| Fichier | Rôle |
|---|---|
| [`Dockerfile`](../../Dockerfile) | Compile le binaire et produit l'image finale |
| [`compose.yaml`](../../compose.yaml) | L'application et PostgreSQL |
| [`compose.bank.yaml`](../../compose.bank.yaml) | À ajouter pour la connexion bancaire (clé EnableBanking montée en secret) |
| [`compose.dev.yaml`](../../compose.dev.yaml) | Développement : publie PostgreSQL sur `127.0.0.1:5433` |
| [`.env.example`](../../.env.example) | Les variables du compose, à copier vers `.env` (ignoré par Git) |
| [`.dockerignore`](../../.dockerignore) | Ce qui n'entre jamais dans l'image : secrets, bases, documentation, `target` |
| [`scripts/sqlite-to-postgres.py`](../../scripts/sqlite-to-postgres.py) | Reprend une ancienne base SQLite |

## Démarrer

```bash
cp .env.example .env              # renseigner POSTGRES_PASSWORD : openssl rand -hex 24
docker compose up -d --build
docker compose logs app           # le code de première configuration y est affiché
```

Puis http://localhost:3000, page `/setup` : le code, puis le mot de passe ([Sécurité](security.md#authentification)). L'application crée son schéma elle-même (migrations embarquées) à chaque démarrage.

Avec la banque : mettre la clé dans `secrets/enablebanking.pem` (droits `0600`), renseigner `WORTHFOLD_ENABLEBANKING_APP_ID` et `COMPOSE_FILE=compose.yaml:compose.bank.yaml` dans `.env`, puis relancer. L'adresse de retour déclarée chez EnableBanking doit être celle de `WORTHFOLD_BANK_REDIRECT_URL` (par défaut `http://localhost:3000/bank/callback`).

## L'image

Deux étapes :

1. **Compilation** (`rust:alpine`) : `cargo build --release --locked` pour la cible musl, donc un **binaire statique**. Le profil release (`Cargo.toml`) active LTO complet, un seul `codegen-unit`, `panic = "abort"` et le retrait des symboles. Tailwind est téléchargé en version figée (`TAILWIND_VERSION`) avec **vérification de la somme de contrôle**. Les caches de Cargo vivent dans BuildKit (`--mount=type=cache`), pas dans l'image : un changement de code ne recompile pas les dépendances.
2. **Image finale** (`distroless/static:nonroot`) : le binaire, les certificats pour joindre EnableBanking, et rien d'autre : ni shell, ni gestionnaire de paquets, ni outil réseau. Utilisateur non-root.

Conséquences : une surface d'attaque minuscule, une image de quelques mégaoctets, un démarrage immédiat. Comme il n'y a pas de shell, la sonde de santé est le binaire lui-même (`worthfold healthcheck`, qui interroge `/login`).

Le contexte de build exclut les secrets (`.env`, `secrets/`, `*.pem`, `*.db`), la documentation et `.git` : rien de secret ne peut entrer dans l'image, même par erreur.

## Le compose

### Application (`app`)

| Réglage | Pourquoi |
|---|---|
| `depends_on: db: service_healthy` | N'attend pas une base qui démarre : l'application ne se lance que quand `pg_isready` répond |
| Port publié sur **`127.0.0.1`** seulement | Joignable depuis la machine, pas depuis le réseau |
| `read_only: true`, `cap_drop: [ALL]`, `no-new-privileges` | L'application n'écrit nulle part (tout est en base) et n'a besoin d'aucun privilège |
| `user: ${WORTHFOLD_UID}:${WORTHFOLD_GID}` | L'utilisateur de la machine : il lit la clé de la banque montée en `0600` sans qu'on en élargisse les droits |
| `mem_limit: 256m`, `pids_limit` | Un défaut ne peut pas épuiser la machine |
| `restart: unless-stopped` | Relancé après une panne (`panic = "abort"`) ou un redémarrage |
| Journaux `json-file` bornés | 3 fichiers de 10 Mo : les journaux ne remplissent pas le disque |

### Base (`db`)

| Réglage | Pourquoi |
|---|---|
| Réseau `backend` en **`internal: true`** | La base n'est joignable que par l'application, et ne peut joindre personne |
| Aucun port publié (hors `compose.dev.yaml`) | Pas d'accès depuis la machine ni le réseau |
| `POSTGRES_PASSWORD` **obligatoire**, sans valeur par défaut | Le compose refuse de démarrer sans mot de passe |
| `--data-checksums` | Une corruption silencieuse du disque se détecte |
| `read_only`, `tmpfs` pour `/tmp` et `/run/postgresql`, capacités réduites à celles de l'entrypoint | Postgres n'écrit que dans son volume |
| Réglages Postgres pour 512 Mo et une personne | 20 connexions (l'application en ouvre 8 au plus), `shared_buffers` 128 Mo, requêtes lentes (> 500 ms) journalisées, client oublié en transaction coupé après 60 s |
| Volume nommé `db-data` | Les données survivent à `docker compose down` (mais pas à `down -v`) |
| `stop_grace_period: 30s` | Laisse Postgres terminer proprement |

## Sauvegarde et restauration

Rien ne sauvegarde automatiquement : c'est à planifier (cron, timer systemd).

```bash
# Sauvegarde : un fichier compressé, daté
docker compose exec -T db pg_dump -U worthfold -d worthfold --no-owner \
  | gzip > "worthfold-$(date +%F).sql.gz"

# Restauration, dans une base VIDE (arrêter l'application d'abord)
docker compose stop app
docker compose exec -T db psql -U worthfold -d postgres -c 'DROP DATABASE worthfold' -c 'CREATE DATABASE worthfold'
gunzip -c worthfold-2026-11-26.sql.gz | docker compose exec -T db psql -U worthfold -d worthfold -v ON_ERROR_STOP=1
docker compose start app
```

Une sauvegarde contient **tout**, y compris les `session_id` bancaires et l'empreinte du mot de passe : la chiffrer ou la ranger comme la base elle-même. Tester une restauration avant d'en avoir besoin.

### Fusionner des opérations en double

Avant le rapprochement automatique des sources ([Transactions](../specs/transactions.md#pas-de-doublons)), un même mouvement lu chez la banque et importé d'un relevé a pu être enregistré deux fois. `scripts/merge-duplicate-transactions.sql` les fusionne avec la même règle que l'application : sur un même compte, même date et même montant, libellés identiques ou dont au moins 80 % des mots du plus court se retrouvent dans l'autre (accents, casse et ponctuation ignorés) ; on garde la ligne du relevé (et on lui reprend la catégorie de l'autre si elle n'en a pas), on supprime celle de la banque. Il ne touche jamais deux opérations de la même source.

Par défaut, il **simule** : il liste ce qu'il fusionnerait, puis annule tout. Après une sauvegarde, `-v apply=1` fusionne pour de bon :

```bash
docker compose exec -T db psql -U worthfold -d worthfold < scripts/merge-duplicate-transactions.sql              # simulation
docker compose exec -T db psql -U worthfold -d worthfold -v apply=1 < scripts/merge-duplicate-transactions.sql  # fusion
```

## Reprendre une ancienne base SQLite

Si tu as des données dans un ancien `worthfold.db` : démarrer le compose (l'application crée le schéma), puis :

```bash
python3 scripts/sqlite-to-postgres.py worthfold.db \
  | docker compose exec -T db psql -U worthfold -d worthfold -v ON_ERROR_STOP=1
```

Le script refuse de s'exécuter si la base contient déjà des données, fait tout dans **une transaction** (une erreur n'importe rien) et n'ouvre le fichier SQLite qu'en lecture. Il ne reprend ni le mot de passe ni les sessions (choisis le mot de passe à `/setup`), ni les consentements en cours ni les opérations en attente (la prochaine lecture de la banque les rend). Vérifié sur une vraie base : le patrimoine net affiché est identique avant et après.

## Les tâches

Elles se lancent dans un conteneur éphémère, sur la même image :

```bash
docker compose run --rm -e WORTHFOLD_BANK_SYNC_TASK=1 app        # lire les comptes bancaires
docker compose run --rm -e WORTHFOLD_RESET_PASSWORD_TASK=1 app   # mot de passe oublié
```

Voir [Tâches](tasks.md). La première se planifie (au plus quelques fois par jour : les banques limitent les lectures).

## Intégration et publication (GitHub Actions)

- **`.github/workflows/ci.yml`** (push et pull request sur `main`) : `cargo fmt --check`, `cargo clippy` sans avertissement, `cargo nextest run` contre un vrai PostgreSQL 17 (service du job, `WORTHFOLD_TEST_DATABASE_URL` définie : les tests de persistance s'exécutent), et un build de l'image Docker sans publication. Tailwind est installé dans la version du `Dockerfile`, empreinte vérifiée.
- **`.github/workflows/release.yml`** (push sur `main`) : `semantic-release` calcule la version à partir des messages de commit (convention *Conventional Commits* : `feat:`, `fix:`…) et crée la release GitHub ; s'il y en a une, l'image est publiée sur `ghcr.io/<propriétaire>/worthfold` avec les tags `X.Y.Z`, `X.Y`, `X` et `latest`. Sans commit `feat:` ou `fix:`, aucune version n'est publiée. Configuration dans `.releaserc.json`.
- **`.github/dependabot.yml`** : mises à jour hebdomadaires de Cargo, de l'image de base Docker et des actions.
- L'image n'est construite que pour `linux/amd64`. Pour un cluster ARM, ajouter `platforms: linux/amd64,linux/arm64` à l'étape de build (le `Dockerfile` gère les deux).
- Le déploiement lui-même (manifestes Kubernetes, Flux) est hors du dépôt applicatif : Flux suit les tags semver de l'image.

## Mettre à jour

```bash
git pull && docker compose up -d --build
```

Les migrations s'appliquent au démarrage. Faire une sauvegarde avant. Une migration appliquée ne se modifie jamais.

## Exposer hors de la machine

Le compose publie sur `127.0.0.1` à dessein. Pour ouvrir l'application à d'autres machines, mets un proxy qui termine HTTPS devant (Caddy, Traefik, un Ingress) et définis **`WORTHFOLD_PUBLIC_URL`** : l'adresse que tape le navigateur, sans chemin (`https://worthfold.example.com`). Elle fait trois choses :

- la garde des requêtes accepte cet hôte en plus de `127.0.0.1` et `localhost` (tout autre reste refusé : DNS rebinding) ;
- en `https`, les cookies portent `Secure` et les réponses `Strict-Transport-Security` ;
- l'adresse de retour de la banque devient `<adresse>/bank/callback` (à déclarer chez EnableBanking) ; `WORTHFOLD_BANK_REDIRECT_URL` la remplace si besoin.

Le proxy doit transmettre le `Host` d'origine (c'est le cas par défaut de Traefik, Caddy et ingress-nginx). Une adresse en `http` est acceptée mais n'a aucun `Secure` : réserve-la à un réseau de confiance.

**Le frein des essais de mot de passe est par adresse IP** (voir [Sécurité](security.md)). Derrière un proxy, déclare combien il y en a avec `WORTHFOLD_PROXY_HOPS` (1 pour un Ingress ou un Caddy seul, 2 avec un répartiteur de charge devant) : sans cela, tous les clients ont l'adresse du proxy et partagent un seul frein. Ne mets jamais plus de proxys que la réalité : l'adresse retenue serait alors une valeur écrite par le client. Sur Kubernetes, l'Ingress doit garder l'IP source (`externalTrafficPolicy: Local` sur son Service).

Reste hors de portée : une double authentification.

## Kubernetes et Flux

Les manifestes sont dans [`kubernetes/`](../../kubernetes/), sur le modèle de ceux d'eosa. `kubectl apply -k kubernetes/`, ou une `Kustomization` Flux dont le `path` est `./kubernetes`.

| Fichier | Contenu |
|---|---|
| `deployment.yaml`, `service.yaml` | L'application : une instance (stratégie `Recreate`), non-root, système de fichiers en lecture seule, sonde `/worthfold healthcheck` (une sonde `httpGet` enverrait l'IP du pod comme `Host`, que la garde refuse), service `ClusterIP` |
| `postgres.yaml` | PostgreSQL 17 (StatefulSet, volume de 2 Gi, réglage identique au compose) et son service |
| `networkpolicy.yaml` | Seuls l'application et la tâche de lecture atteignent la base |
| `bank-sync-cronjob.yaml` | Lecture de la banque toutes les 6 h (`WORTHFOLD_BANK_SYNC_TASK`) |
| `config.yaml.example`, `secret.yaml.example`, `postgres-secret.yaml.example` | Réglages et secrets à copier, remplir et chiffrer (SOPS, SealedSecret) : jamais de vraies valeurs dans le dépôt |
| `ingress.yaml.example` | Un Ingress HTTPS, à adapter à ton contrôleur et à ton nom de domaine |

Les clés du ConfigMap et du Secret portent le nom des variables d'environnement (`envFrom`). La clé privée EnableBanking est la clé `enablebanking.pem` du Secret `worthfold-secrets`, montée en `/run/secrets/enablebanking/enablebanking.pem` : renseigne alors `WORTHFOLD_ENABLEBANKING_APP_ID` et `WORTHFOLD_ENABLEBANKING_KEY_PATH` (ce chemin) dans le ConfigMap, ou laisse les deux vides.

Première mise en service :

1. Crée les secrets, le ConfigMap (avec `WORTHFOLD_PUBLIC_URL`) et l'Ingress à partir des exemples.
2. Après le premier démarrage : `kubectl logs deploy/worthfold` affiche le code de première configuration ; ouvre `/setup` à l'adresse publique.
3. Reprise de l'ancienne base : `kubectl port-forward svc/worthfold-postgres 5433:5432`, puis le script `scripts/sqlite-to-postgres.py` (voir plus haut).
4. Sauvegarde : `kubectl exec statefulset/worthfold-postgres -- sh -c 'pg_dump -U "$POSTGRES_USER" worthfold' | gzip > worthfold-$(date +%F).sql.gz`, à planifier.

Version : les manifestes utilisent `ghcr.io/noynto/worthfold:latest`. Pour suivre les versions, décommente `images:` dans `kubernetes/kustomization.yaml` (avec l'automatisation d'images de Flux, `ImageRepository` + `ImagePolicy` en `semver`, le commentaire `$imagepolicy` indique où écrire le tag).
