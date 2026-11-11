# Écrans

Application des [fondations](foundations.md) aux écrans de worthfold. Templates : `crates/server/templates/`.

## Structure commune (`base.html`)

- Fond `surface`, police Roboto Flex, texte `body-large`.
- Une colonne centrée (`max-w-3xl`), cartes empilées avec `gap-4`.
- En-tête : le nom « worthfold » (`title-large`, `primary`, lien vers le tableau de bord) et la **navigation** : deux destinations en pilules (« Tableau de bord », « Actifs »), de 48 px de haut. La page active porte l'indicateur `secondary-container` et `aria-current="page"` ; la page d'un actif active « Actifs ».

## Consulter, parcourir, agir

Selon la [règle d'UX 7](ux.md#7-lessentiel-dabord-le-détail-à-la-demande), ce qu'on consulte souvent et ce qu'on fait de temps en temps sont séparés :

- le **tableau de bord** (`/`) sert à consulter : aucun formulaire ;
- la page **Actifs** (`/assets`) sert à parcourir les actifs et à en ajouter un ;
- la page **d'un actif** (`/assets/{id}`) sert à agir sur lui : le valoriser (action principale), voir son historique, le modifier, le supprimer.

## Tableau de bord (`dashboard.html`)

De haut en bas, il tient sur un écran de téléphone :

1. **Héros** : « Patrimoine net au 09/11/2026 » et le total, en `display-medium` sur mobile et `display-large` sur ordinateur, avec emphase (`font-medium`) et chiffres tabulaires. Surface `primary-container`, coins `xl-increased`. S'il est négatif, la surface passe en `error-container`. S'il n'est pas calculable, il affiche « — » et un bandeau explique pourquoi.
2. **Carte « Évolution »** : le graphique, sur `surface-container-low`, coins `xl`. Il arrive par un fragment htmx : en attendant, un squelette `animate-pulse` en `surface-container` occupe exactement sa place (même rapport d'aspect), pour que rien ne saute.
3. **Carte « Actifs »** (`partials/summary.html`) : résumer sans détailler.
   - Le nombre d'actifs et un bouton tonal « Gérer les actifs ».
   - **Avoirs** et **dettes**, deux montants (`title-large`) sur `surface-container`, coins `lg` ; les dettes en `error` quand il y en a.
   - **À mettre à jour** : les actifs dont la valeur a plus de 90 jours (« valeur du 10/06/2026 ») ou qui n'en ont jamais eu (« jamais valorisé »), en `tertiary`. Chaque ligne mène à la page de l'actif. Sans rien à signaler : « Toutes les valeurs ont moins de 90 jours ».
   - Sans aucun actif : une phrase d'explication et un bouton plein « Ajouter un actif », vers le formulaire de la page Actifs (règle 9).

## Actifs (`assets_page.html`)

1. **Titre** « Actifs » (`headline-small`).
2. **Bandeau** d'erreur d'un formulaire envoyé sans JavaScript, en `error-container`, `role="alert"`.
3. **Liste** (`partials/assets.html`) : une ligne par actif, séparée par `outline-variant`. **Chaque ligne entière est un lien** vers la page de l'actif (couche d'état au survol, chevron `›`) : la liste présente, elle ne sert pas à saisir.
   - Nom (`title-medium`) et type (`body-medium`, discret). Un nom long passe à la ligne, il n'est jamais coupé.
   - Dernière valeur (`title-medium`, `error` si négative), sa date (`label-medium`) et, si besoin, « à mettre à jour » ou « jamais valorisé » en `tertiary` ([UX](ux.md), règle 8).
4. **Carte « Ajouter un actif »** (`id="add"`) : nom, type, devise (`field` avec `field-label`) et bouton plein « Ajouter ». Erreur sous le formulaire ; après un succès, le formulaire se vide et le nouvel actif apparaît dans la liste.

## Retours communs

**Snackbar** (toutes les pages, `base.html`) : la confirmation d'une action réussie (« Valorisation de 3 300,00 € enregistrée », « « Maison » supprimé »), y compris après une modification ou une suppression qui change de page (message flash), centrée en bas de l'écran, sur `inverse-surface`. Elle entre avec le ressort `spatial-fast`, reste 4 secondes et s'efface seule ; la zone qui l'accueille est permanente et `role="status"`, pour être annoncée par les lecteurs d'écran.

**Pendant un envoi**, le bouton du formulaire est désactivé (opacité réduite, curseur d'attente).

**Page de message** (`message_page.html`) : quand il n'y a rien d'autre à montrer (« Actif introuvable », « Page introuvable », « Page indisponible »). Une carte `surface-container-low` avec un titre `headline-small`, la raison en `body-large` `on-surface-variant`, et un seul bouton plein pour repartir : la liste des actifs, ou le tableau de bord après une panne. Jamais de page blanche.

## Page d'un actif (`asset_page.html`)

De haut en bas, l'action principale juste sous la valeur actuelle :

1. **Retour** « ← Tous les actifs » (l'onglet « Actifs » reste actif).
2. **En-tête** (`partials/asset_header.html`, `xl-increased`) : le type (`label-large`), le nom (`headline-small`), la dernière valeur en grand (`display-medium`, `error` si négative), sa date et sa fraîcheur.
3. **Nouvelle valorisation** : montant (« Capital restant dû » pour un prêt) et date (limitée au jour même), avec des libellés visibles, et le bouton plein « Enregistrer ». Un refus s'affiche sous le formulaire et la saisie reste ; un succès vide le montant, met à jour l'en-tête et l'historique sans recharger la page, et confirme dans la snackbar.
4. **Historique** (`partials/asset_history.html`) : toutes les valorisations, de la plus récente à la plus ancienne. Chacune montre sa date, sa valeur et l'écart avec la précédente (« +2 450,00 € », en `error` s'il est négatif, « inchangée » s'il est nul). Pour un prêt, un remboursement donne un écart positif : la dette baisse, le patrimoine monte. Vide : « Aucune valorisation pour l'instant : saisis la première ci-dessus ».
   - Chaque ligne a un bouton texte « Supprimer » (`btn-text-error`, nom accessible « Supprimer la valorisation du … ») pour corriger une saisie erronée, avec une confirmation qui cite la date et le montant. Un refus s'affiche en haut de l'historique ; un succès met à jour l'en-tête, l'historique et les sections de gestion sans recharger la page.
5. **Modifier le nom, le type ou la devise** (`partials/asset_manage.html`) : section repliée (`<details>`, secondaire). Une devise figée s'affiche en texte, avec la raison. Après un succès, la page se recharge avec la confirmation.
6. **Supprimer** : ce qui sera perdu, chiffré et accordé (« son historique : 3 valorisations », « aucune valorisation »), et bouton `btn-outlined-error`, avec confirmation par `hx-confirm` (voir [Web](../tech/web.md#sécurité)). Après un succès, retour à la page Actifs, avec la confirmation dans la snackbar.

En-tête, historique et sections de gestion se rechargent ensemble après une valorisation : le nombre de valorisations annoncé et le verrou de la devise restent justes sans recharger la page.

## Graphique

Le SVG (`crates/server/src/chart.rs`) est intégré à la page par htmx et utilise les mêmes jetons (sans JavaScript, il est servi seul et charge `/static/app.css`) :

- courbe en escalier et aire en `primary` (aire à 12 %), points en `primary` entourés de la couleur de la carte ;
- grille en `outline-variant`, ligne du zéro en `outline` pointillé ;
- graduations et dates en `on-surface-variant`, police Roboto Flex, chiffres tabulaires ;
- au survol, un point grossit avec le ressort `spatial-fast`.

Fond transparent : la carte qui l'accueille fournit la surface. Le SVG déclare `color-scheme: light dark`, sans quoi, servi seul, le navigateur lui donnerait un fond clair opaque en thème sombre.

Lisibilité sur téléphone : le graphique est dessiné sur 720 unités de large et réduit à environ 330 px. Une media query de ses styles (écran ≤ 560 px) agrandit graduations, dates et points pour garder environ 11 px affichés, et les traits ont une épaisseur constante (`vector-effect: non-scaling-stroke`).

Servi seul, le SVG est du XML : aucun `<` ni `&` dans ses styles, même en commentaire, sinon tout le document est rejeté (un test le vérifie). Intégré, ses styles deviennent globaux : ses classes sont préfixées `chart-`.
