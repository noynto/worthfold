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
| `GET` | `/fragments/banks?country=FR` | `ListBanks` | Fragment : les banques connectables du pays, en `option` d'une `datalist` (vide si le pays est incomplet ou l'agrégateur en panne) |
| `GET` | `/net-worth/chart.svg` | `ReadNetWorthHistory` | Le graphique en document SVG autonome (page sans JavaScript) |
| `POST` | `/assets` | `AddAsset` | Voir [Formulaires](#formulaires) |
| `GET` | `/assets/{id}` | `ReadAsset` | Page d'un actif : valeur, nouvelle valorisation, historique, modification, suppression (page « Actif introuvable », 404, si l'actif n'existe pas) |
| `GET` | `/fragments/assets/{id}` | `ReadAsset` | Fragment : l'en-tête, l'historique et les sections de gestion d'un actif |
| `POST` | `/assets/{id}` | `UpdateAsset` | Voir [Formulaires](#formulaires) |
| `POST` | `/assets/{id}/delete` | `DeleteAsset` | Voir [Formulaires](#formulaires) |
| `GET` | `/assets/{id}/transactions` | `ReadTransactions` | Opérations d'un compte bancaire (les 200 plus récentes) et formulaire d'import (page « Compte introuvable », 404, si l'actif n'existe pas) |
| `POST` | `/assets/{id}/transactions/{transaction_id}/category` | `CategorizeTransaction` | Classer une opération (liste vide : retirer la catégorie) ; redirige vers la ligne (`#t-{id}`), sans htmx |
| `GET` | `/login` | — | Page de connexion (sans navigation) ; redirige vers `/setup` tant qu'aucun mot de passe n'existe |
| `POST` | `/login` | `Login` | Ouvrir une session (cookie `worthfold_session`), puis le tableau de bord ; un refus revient sur `/login?error=` |
| `GET` | `/setup` | — | Première configuration : code affiché dans la console, mot de passe, confirmation |
| `POST` | `/setup` | `SetupPassword` | Enregistrer le premier mot de passe et ouvrir la session |
| `POST` | `/logout` | `Logout` | Fermer la session (côté serveur aussi) et revenir à `/login` |
| `GET` | `/account` | — | Page Compte : changer le mot de passe, se déconnecter |
| `POST` | `/account/password` | `ChangePassword` | Changer le mot de passe : ferme toutes les autres sessions |
| `GET` | `/categories` | `ReadCategories` | Page Catégories : catégories, règles, formulaires d'ajout |
| `POST` | `/categories` | `AddCategory` | Ajouter une catégorie, sans htmx |
| `POST` | `/categories/defaults` | `AddDefaultCategories` | Ajouter les catégories usuelles, sans htmx |
| `POST` | `/categories/{id}/delete` | `DeleteCategory` | Supprimer une catégorie et ses règles |
| `POST` | `/categories/rules` | `AddCategoryRule` | Ajouter une règle, sans htmx |
| `POST` | `/categories/rules/{id}/delete` | `DeleteCategoryRule` | Supprimer une règle |
| `POST` | `/categories/apply` | `ApplyCategoryRules` | Classer les opérations sans catégorie d'après les règles |
| `GET` | `/bank` | `ReadBankConnections` | Page Banque : consentements, jours restants, comptes à rattacher, formulaire de connexion |
| `GET` | `/banks` | `ReadInstitutions` | La page Banques : leur format CSV et leurs comptes |
| `POST` | `/banks` | `AddInstitution` | Ajouter une banque avec son format CSV |
| `POST` | `/banks/{name}/update` | `UpdateInstitution` | Remplacer le format CSV d'une banque (son nom ne change pas) |
| `POST` | `/banks/{name}/delete` | `DeleteInstitution` | Supprimer une banque (refusé si des comptes s'y rattachent) |
| `POST` | `/bank/connect` | `StartBankConnection` | Démarrer un consentement : redirige vers la banque |
| `GET` | `/bank/callback` | `CompleteBankConnection` | Retour de la banque (code et état) ; un refus ou un état inconnu revient sur `/bank` avec un message |
| `POST` | `/bank/accounts/{uid}/link` | `LinkBankAccount` | Rattacher (ou détacher, avec une valeur vide) un compte de la banque à un compte de worthfold |
| `POST` | `/bank/sync` | `SyncBankAccounts` | Lire les opérations et soldes des comptes rattachés |
| `POST` | `/bank/sync-all` | `SyncBankAccounts` (`full`) | Relire les 90 derniers jours, sans tenir compte de la dernière lecture |
| `POST` | `/bank/{id}/delete` | `DeleteBankConnection` | Retirer un consentement (confirmation htmx) |
| `GET` | `/forecast` | `ReadForecast` | Page Prévisions : solde et patrimoine projetés à 3, 6 et 12 mois, mois après mois, alertes |
| `GET` | `/payments` | `ReadPlannedPayments` | Page Paiements : calendrier des échéances, suggestions détectées, paiements, formulaire d'ajout |
| `POST` | `/payments` | `AddPlannedPayment` | Ajouter un paiement, sans htmx |
| `POST` | `/payments/suggestions` | `AcceptRecurrenceSuggestion` | Ajouter une suggestion détectée (rythme et montant recalculés côté serveur) |
| `POST` | `/payments/{id}/delete` | `DeletePlannedPayment` | Supprimer un paiement (confirmation htmx) |
| `POST` | `/payments/{id}/steps` | `AddAmountStep` | Ajouter un palier de montant |
| `POST` | `/payments/{id}/occurrences/{date}` | `SetOccurrenceException` | Sauter une échéance, en changer le montant (`action=amount`) ou la rétablir (`action=clear`) |
| `GET` | `/budget`, `/budget/{mois}` | `ReadBudget` | Page Budget d'un mois (`2026-10`) : synthèse, enveloppes, épargne ; le mois en cours par défaut, page « Mois introuvable » (404) si l'adresse n'est pas un mois |
| `POST` | `/budget/{mois}/envelopes` | `SetEnvelope` | Fixer un plafond à partir du mois, sans htmx |
| `POST` | `/budget/{mois}/envelopes/{category_id}/remove` | `SetEnvelope` | Retirer une enveloppe à partir du mois (confirmation htmx) |
| `POST` | `/budget/{mois}/savings` | `SaveSavingsPlan` | Régler l'épargne : livret, plafond, matelas facultatif, sans htmx |
| `POST` | `/assets/{id}/transactions/import` | `ImportTransactions` | Import d'un relevé CSV, voir [Import d'un relevé](#import-dun-relevé) |
| `POST` | `/assets/{id}/valuations` | `RecordValuation` | Voir [Formulaires](#formulaires) |
| `POST` | `/assets/{id}/valuations/{valuation_id}/delete` | `DeleteValuation` | Voir [Formulaires](#formulaires) |
| `GET` | `/static/app.css` | — | Feuille de style compilée par Tailwind, embarquée (`no-cache`) |
| `GET` | `/static/htmx-2.0.11.min.js` | — | htmx, embarqué ; la version est dans le chemin (cache d'un an) |
| `GET` | `/static/app.js` | — | Le seul script de l'application : remet un formulaire à zéro après un succès (`data-reset-on-success`, `data-clear-on-success`) ; `no-cache` |
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

## Catégories

Les formulaires de la page Catégories et le classement d'une opération sont des formulaires classiques, **sans htmx** : un succès redirige avec une confirmation en message flash, un refus rouvre la page avec `?error=`. Les deux suppressions (catégorie, règle) utilisent htmx pour demander une confirmation (`hx-confirm`) ; un refus s'affiche sous le titre de la page (`#error-categories`) et un succès redirige (`HX-Redirect`).

## Paiements

Le formulaire d'ajout est classique, **sans htmx** : tous les champs arrivent en texte (`PaymentForm`) et le handler les lit en français (`parse_form`) avant d'appeler le cas d'usage ; un champ répété est refusé par la désérialisation (`serde_urlencoded`). Le rythme choisi décide des champs lus : les autres sont ignorés. La plage de mois d'« certains mois de l'année » peut passer le 31 décembre. La suppression d'un paiement demande confirmation par htmx (`hx-confirm`).

## Budget

Le mois est dans l'adresse (`/budget/2026-10`) : un refus de formulaire revient sur le même mois. Les formulaires sont classiques, **sans htmx** (succès : redirection avec message flash ; refus : `?error=`), sauf le retrait d'une enveloppe, qui demande confirmation par `hx-confirm`. Les navigations vers le mois précédent et suivant sont de simples liens (le mois suivant n'est proposé que jusqu'au mois en cours).

La jauge d'une enveloppe est un `<progress>` natif, coloré par les classes `meter`, `meter-near` et `meter-over` (`app.css`).

## Import d'un relevé

Le formulaire d'import (`transactions_page.html`) envoie le fichier en `multipart/form-data` (fonctionnalité `multipart` d'axum) et **n'utilise pas htmx** : il marche sans JavaScript, et répond comme un formulaire sans htmx (voir ci-dessus) : `303` vers la page du compte avec la confirmation en message flash, ou avec `?error=` en cas de refus.

- Les champs sont `statement` (le fichier) et `closing_balance` (le solde après la dernière opération, facultatif).
- Le corps est limité à 2 Mo (limite par défaut d'axum), très au-dessus d'un relevé mensuel.
- Le fichier est lu en UTF-8 ; s'il n'est pas valide, en Windows-1252, l'encodage de nombreuses banques françaises (`decode_statement`).

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
- `standalone` en fait un document autonome pour `/net-worth/chart.svg`, qui charge lui-même `/static/app.css` ; les règles `chart-*` sont dans cette feuille, le SVG n'embarque aucun `<style>` ;
- ses styles deviennent globaux une fois intégrés : toutes ses classes sont préfixées `chart-` (Tailwind a déjà une classe `grid`) ;
- la courbe est **en escalier**, parce qu'entre deux saisies le patrimoine vaut la dernière valeur connue (voir [Patrimoine net](../specs/net-worth.md)) ;
- les graduations sont « rondes » (1, 2 ou 5 × 10ⁿ), environ cinq par graphique, avec une ligne pointillée pour le zéro quand il est dans l'échelle.

## Authentification

Un seul cas d'usage répond à la question « suis-je connecté ? » : `CheckSession`, appelé par le middleware `auth.rs`, **après** la garde des requêtes. Sans session valide, tout mène à `/login` (ou à `/setup` tant qu'aucun mot de passe n'existe), sauf `/static/`, `/login` et `/setup`. Une requête htmx sans session reçoit `401` avec `HX-Redirect: /login` : elle ne peut pas suivre une redirection de page. Une session ouverte écarte de `/login` et `/setup`. Le cookie, ses attributs et ses limites : [Sécurité](security.md#authentification).

## En-têtes et contenu de page

Toute réponse porte une CSP stricte, `nosniff`, `X-Frame-Options`, `Referrer-Policy` et, hors `/static/`, `Cache-Control: no-store` (`headers.rs`). Conséquences pour les templates : aucun `<script>` ni `<style>` en ligne, aucun attribut `style=`, `onclick=` ou `hx-on` (htmx est configuré sans évaluation de code ni style injecté). Un comportement qui demande du script va dans `assets/js/app.js`, par attribut `data-*`. Un test (`les_pages_n_ont_ni_script_ni_style_en_ligne`) le vérifie sur les pages.

## Sécurité

Document dédié : [Sécurité](security.md) (modèle de menace, garde des requêtes, échappement, secrets, écarts connus).
