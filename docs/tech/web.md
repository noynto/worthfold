# Web : routes, templates, htmx

Les pages sont rendues côté serveur avec askama ; **htmx** fait les mises à jour partielles (formulaires sans rechargement, fragments qui se rechargent seuls). Tout reste utilisable sans JavaScript : chaque formulaire garde `method="post"` et `action`.

## Routes

| Méthode | Chemin | Cas d'usage | Réponse |
|---|---|---|---|
| `GET` | `/` | `ReadNetWorth` | Tableau de bord : total, avoirs et dettes, actifs à mettre à jour (le graphique se charge à part) |
| `GET` | `/assets` | `ReadNetWorth` | Page Actifs : liste (chaque ligne mène à la page de l'actif), formulaire d'ajout |
| `GET` | `/fragments/net-worth` | `ReadNetWorth` | Fragment : le patrimoine net (héros) |
| `GET` | `/fragments/assets` | `ReadNetWorth` | Fragment : la liste des actifs |
| `GET` | `/fragments/chart` | `ReadNetWorthHistory` | Fragment : le graphique, en SVG intégré |
| `GET` | `/net-worth/chart.svg` | `ReadNetWorthHistory` | Le graphique en document SVG autonome (page sans JavaScript) |
| `POST` | `/assets` | `AddAsset` | Voir [Formulaires](#formulaires) |
| `GET` | `/assets/{id}` | `ReadAsset` | Page d'un actif : valeur, nouvelle valorisation, historique, modification, suppression (page « Actif introuvable », 404, si l'actif n'existe pas) |
| `GET` | `/fragments/assets/{id}` | `ReadAsset` | Fragment : l'en-tête, l'historique et les sections de gestion d'un actif |
| `POST` | `/assets/{id}` | `UpdateAsset` | Voir [Formulaires](#formulaires) |
| `POST` | `/assets/{id}/delete` | `DeleteAsset` | Voir [Formulaires](#formulaires) |
| `POST` | `/assets/{id}/valuations` | `RecordValuation` | Voir [Formulaires](#formulaires) |
| `POST` | `/assets/{id}/valuations/{valuation_id}/delete` | `DeleteValuation` | Voir [Formulaires](#formulaires) |
| `GET` | `/static/app.css` | — | Feuille de style compilée par Tailwind, embarquée (`no-cache`) |
| `GET` | `/static/htmx-2.0.11.min.js` | — | htmx, embarqué ; la version est dans le chemin (cache d'un an) |
| `GET` | `/static/fonts/roboto-flex-latin.woff2` | — | Police Roboto Flex, embarquée (cache d'un an) |
| `GET` | toute autre adresse | — | Page « Page introuvable » (404) |

Une action qui modifie des données est toujours un `POST`, jamais un `GET`.

## htmx

- **Version** : htmx 2.0.11, fichier `crates/server/assets/js/htmx-2.0.11.min.js` (licence 0BSD), embarqué dans le binaire : aucun CDN. Pour changer de version : télécharger l'archive depuis le registre npm, vérifier son empreinte (`dist.integrity`), remplacer le fichier et mettre à jour le chemin dans `assets.rs`, `lib.rs` et `base.html`.
- **Configuration** (`<meta name="htmx-config">` de `base.html`) : une réponse 422 ou 500 est affichée dans la cible (c'est un message d'erreur), une réponse 204 ne change rien.
- **Pages composées** : le handler d'une page n'appelle qu'un cas d'usage. Une information d'un autre cas d'usage se charge par un fragment (`hx-get`, `hx-trigger="load"`) avec un squelette `animate-pulse` à sa forme : c'est le cas du graphique. Sans JavaScript, le graphique est remplacé par `<noscript><object data="/net-worth/chart.svg">`.
- **Fragments qui se rechargent** : le total, la liste des actifs et le graphique écoutent l'événement `worth-changed` (`hx-trigger="worth-changed from:body"`, `hx-swap="outerHTML"`). Chaque fragment est rendu par le même template que la page (`{% include "partials/…" %}`) et porte lui-même ses attributs htmx, pour continuer d'écouter après remplacement. Sur la page d'un actif, l'en-tête, l'historique et les sections de gestion sont séparés par le formulaire de valorisation : chacun redemande le même fragment (`/fragments/assets/{id}`) et n'en garde que sa partie (`hx-select="#asset-header"`, `#asset-history`, `#asset-manage`).
- **Confirmation** : `hx-confirm` sur le formulaire de suppression (boîte du navigateur).

## Formulaires

Chaque formulaire a `method="post"`, `action` et `hx-post` vers la même route, et une cible pour ses erreurs : un paragraphe `role="alert"` vide (masqué par `empty:hidden`) placé sous lui. Le handler regarde l'en-tête `HX-Request` (`Reply` dans `handlers/reply.rs`) :

| Résultat | Avec htmx | Sans JavaScript |
|---|---|---|
| Valorisation, suppression d'une valorisation ou ajout réussi | `200` + `HX-Trigger: worth-changed` : le message d'erreur est vidé, les fragments se rechargent, le formulaire d'ajout se réinitialise, et la confirmation s'affiche dans la snackbar (échange hors cible, `hx-swap-oob`, `partials/snackbar.html`) | `303` vers la page du formulaire, avec la confirmation en message flash |
| Modification réussie | `200` vide + `HX-Redirect` vers la page de l'actif, avec la confirmation en message flash | `303` vers la page de l'actif, avec la confirmation en message flash |
| Suppression réussie | `200` vide + `HX-Redirect: /assets`, avec la confirmation en message flash | `303` vers `/assets`, avec la confirmation en message flash |
| Refus métier ou saisie illisible | `422` avec le message (`partials/inline_error.html`), affiché sous le formulaire ; **la saisie reste en place** | `303` vers la page du formulaire avec `?error=…`, affiché en haut |
| Panne | `500` avec un message générique, affiché sous le formulaire ; détail journalisé | `500` avec un message générique |

**Message flash** (`flash.rs`) : quand une action redirige, sa confirmation la suit dans un cookie de courte durée (`worthfold_flash`, une minute, `HttpOnly`, `SameSite=Strict`, valeur encodée). La page d'arrivée l'affiche dans sa snackbar et efface le cookie dans la même réponse : le message ne s'affiche qu'une fois, ne reste pas dans l'adresse et ne revient pas au rechargement. Les pages qui reçoivent des redirections le lisent : la page Actifs et la page d'un actif.

Une valorisation ne recharge donc jamais la page : seuls le total, la liste et le graphique sont redemandés. Pendant l'envoi, `hx-disabled-elt="find button"` désactive le bouton du formulaire.

## Erreurs

- **Refus** (`UseCaseError::Refusal`) : le message en français du cas d'usage, montré tel quel (voir [Formulaires](#formulaires)).
- **Saisie illisible** (montant, date, type inconnu) : même traitement, avec un message rédigé par le handler.
- **Panne** (`UseCaseError::Technical`) : journalisée avec `tracing`, réponse 500. Une page affiche « Page indisponible » (`message_page.html`) avec un lien vers le tableau de bord ; avec htmx, un message court dans la cible d'erreur.
- **Introuvable** (actif supprimé, adresse inconnue) : la même page de message en 404, qui dit pourquoi et mène à la liste des actifs. Jamais une page blanche.
- **Fragment** : une erreur ne casse jamais la page. Le graphique affiche le message à la place de la courbe ; un autre fragment en panne laisse un court message à sa place.
- **Une page ne redirige jamais vers elle-même** en cas d'erreur, ce qui ferait une boucle infinie. Un total incalculable est affiché comme tel, avec la liste des actifs.

## Templates

- askama, dans `crates/server/templates/` : `base.html` (structure commune, navigation, htmx, zone de la snackbar), `dashboard.html`, `assets_page.html`, `asset_page.html`, et les fragments dans `partials/` (`net_worth.html`, `summary.html`, `assets.html`, `asset_header.html`, `asset_history.html`, `asset_manage.html`, `asset_detail.html` qui réunit les trois précédents, `chart.html`, `inline_error.html`, `snackbar.html`). Chaque page indique sa rubrique (`section`) pour la navigation.
- Les templates sont **compilés et vérifiés à la compilation** : un champ manquant ou mal nommé ne compile pas. Un fragment inclus par une page utilise les mêmes noms de champs dans la struct de la page et dans celle du fragment.
- Le handler prépare une struct de vue avec des chaînes déjà mises en forme ; le template n'a aucune logique métier.
- Styles : classes Tailwind et composants du design system ([Fondations](../design/foundations.md), [Écrans](../design/screens.md)), thème sombre automatique.
- **Aucun JavaScript écrit à la main**, à une exception près : `hx-on::after-request` vide un formulaire après un succès (ajout d'un actif, montant d'une valorisation). Il n'y insère aucune valeur.
- **Réponse de succès sans espaces** : la cible d'un formulaire (sa zone d'erreur) doit recevoir un contenu vraiment vide, sinon `empty:hidden` ne la masque plus ; `Reply::worth_changed` rogne donc la réponse.

## Mise en forme des montants

`crates/server/src/format.rs`, à la française :

- `money` : `12 345,67 €`. Espace fine insécable entre les milliers, virgule décimale, toujours deux décimales (arrondi au centime le plus proche), symbole après le montant avec une espace insécable, vrai signe moins (`−`).
- `money_short`, pour les axes du graphique : `250 k€`, `1,2 M€`.
- Les montants négatifs sont affichés en `error`.
- À la saisie, `1 234,56`, `1234.56` et `245 000` sont acceptés.

La mise en forme est de la présentation : le domaine garde des valeurs exactes et n'en sait rien.

## Graphique

`crates/server/src/chart.rs` dessine l'évolution du patrimoine en SVG, côté serveur, sans bibliothèque :

- `render` produit un `<svg>` à intégrer : le fragment `/fragments/chart` le place dans la page, qui fournit la feuille de style ; les infobulles (date et total exact au survol d'un point) fonctionnent ;
- `standalone` en fait un document autonome pour `/net-worth/chart.svg`, qui charge lui-même `/static/app.css` ;
- ses styles deviennent globaux une fois intégrés : toutes ses classes sont préfixées `chart-` (Tailwind a déjà une classe `grid`) ;
- la courbe est **en escalier**, parce qu'entre deux saisies le patrimoine vaut la dernière valeur connue (voir [Patrimoine net](../specs/net-worth.md)) ;
- les graduations sont « rondes » (1, 2 ou 5 × 10ⁿ), environ cinq par graphique, avec une ligne pointillée pour le zéro quand il est dans l'échelle.

## Sécurité

- **Écoute locale uniquement** (`127.0.0.1`) : les données sont personnelles et l'application n'a pas encore d'authentification. Ne pas l'exposer sur un réseau en l'état.
- **Garde des requêtes** (`guard.rs`, devant toutes les routes) : écouter en local ne suffit pas, car une page d'un autre site ouverte dans le même navigateur peut appeler le serveur local. Le CORS n'y change rien : il empêche de *lire* une réponse, pas d'*envoyer* un formulaire. Deux règles, toute violation répond 403 et est journalisée :
  - **`Host` local** : seuls `127.0.0.1` et `localhost` (avec ou sans port) sont acceptés, même en lecture. Cela bloque le *DNS rebinding*, où un domaine piégé pointe vers 127.0.0.1 pour lire les pages comme s'il était worthfold.
  - **Modification depuis worthfold** : un `POST` n'est accepté que si `Sec-Fetch-Site` vaut `same-origin` (ou `none`), ou s'il est absent (`curl`). Cela bloque le CSRF, un formulaire d'un autre site qui s'envoie tout seul. Une autre application sur `localhost` (`same-site`) est refusée aussi. Un lien venant d'un autre site (`GET`) reste accepté.
- **Échappement** : askama échappe toute valeur insérée dans le HTML, y compris les messages d'erreur renvoyés à htmx (`partials/inline_error.html`) : un message qui cite la saisie (« Le montant « … » n'est pas un nombre ») ne peut pas injecter de HTML. Aucun HTML n'est construit par concaténation en Rust.
- **Seule exception à l'échappement** : `{{ svg|safe }}` dans `partials/chart.html`. Le SVG est produit par `chart.rs`, qui n'y insère que des nombres, des dates et des montants mis en forme, et échappe le texte de `chart::message`. Aucun autre `|safe`.
- **Pas de valeur dans du JavaScript** : l'échappement HTML ne protège pas une valeur placée dans un script (le navigateur la décode avant de l'exécuter). Le message de suppression passe par `hx-confirm`, dont htmx affiche le texte tel quel sans l'exécuter ; aucun attribut `hx-on` ne contient de valeur.
- **SQL** : toujours des paramètres `?`, jamais de valeur concaténée.
