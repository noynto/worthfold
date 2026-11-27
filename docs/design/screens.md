# Écrans

Application des [fondations](foundations.md) aux écrans de worthfold. Templates : `crates/server/templates/`.

## Structure commune (`base.html`)

- Fond `surface`, police Roboto Flex, texte `body-large`.
- Une colonne centrée (`max-w-3xl`), cartes empilées avec `gap-4`.
- En-tête : le nom « worthfold » (`title-large`, `primary`, lien vers le tableau de bord) et la **navigation** : quatre destinations en pilules (« Tableau de bord », « Actifs », « Budget », « Prévisions ») et un menu « Réglages » (« Paiements », « Banques », « Connexion bancaire », « Catégories », « Compte »), de 48 px de haut. Le menu est un `details` : il marche sans JavaScript, et sa pilule est active quand la page courante en fait partie. La page active porte l'indicateur `secondary-container` et `aria-current="page"` ; la page d'un actif active « Actifs ».

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
   - Nom (`title-medium`) et type (`body-medium`, discret), suivi de la banque et du numéro masqué (`•••• 2606`) pour un compte bancaire renseigné. Un nom long passe à la ligne, il n'est jamais coupé.
   - Dernière valeur (`title-medium`, `error` si négative), sa date (`label-medium`) et, si besoin, « à mettre à jour » ou « jamais valorisé » en `tertiary` ([UX](ux.md), règle 8).
4. **Carte « Ajouter un actif »** (`id="add"`) : nom, type et, quand plusieurs devises sont proposées, devise (`field` avec `field-label`) ; tant que seul l'euro est proposé, la devise n'est pas demandée. Sur une seconde ligne, la banque (liste des banques, voir la page Banques) et le numéro de compte ou l'IBAN (facultatifs, pour un compte bancaire ; l'IBAN est vérifié). Bouton plein « Ajouter ». Erreur sous le formulaire ; après un succès, le formulaire se vide et le nouvel actif apparaît dans la liste.

## Retours communs

**Snackbar** (toutes les pages, `base.html`) : la confirmation d'une action réussie (« Valorisation de 3 300,00 € enregistrée », « « Maison » supprimé »), y compris après une modification ou une suppression qui change de page (message flash), centrée en bas de l'écran, sur `inverse-surface`. Elle entre avec le ressort `spatial-fast`, reste 4 secondes et s'efface seule ; la zone qui l'accueille est permanente et `role="status"`, pour être annoncée par les lecteurs d'écran.

**Pendant un envoi**, le bouton du formulaire est désactivé (opacité réduite, curseur d'attente).

**Page de message** (`message_page.html`) : quand il n'y a rien d'autre à montrer (« Actif introuvable », « Page introuvable », « Page indisponible »). Une carte `surface-container-low` avec un titre `headline-small`, la raison en `body-large` `on-surface-variant`, et un seul bouton plein pour repartir : la liste des actifs, ou le tableau de bord après une panne. Jamais de page blanche.

## Page d'un actif (`asset_page.html`)

De haut en bas, l'action principale juste sous la valeur actuelle :

1. **Retour** « ← Tous les actifs » (l'onglet « Actifs » reste actif).
2. **En-tête** (`partials/asset_header.html`, `xl-increased`) : le type (`label-large`, suivi de la banque et du numéro entier, groupé par quatre, pour un compte bancaire renseigné), le nom (`headline-small`), la dernière valeur en grand (`display-medium`, `error` si négative), sa date et sa fraîcheur.
   Pour un **compte bancaire**, une ligne « Opérations du compte › » suit l'en-tête et mène à la page des opérations.
3. **Nouvelle valorisation** : montant, avec le symbole de la devise dans le libellé (« Montant (€) », « Capital restant dû (€) » pour un prêt) et date (limitée au jour même), avec des libellés visibles, et le bouton plein « Enregistrer ». Un refus s'affiche sous le formulaire et la saisie reste ; un succès vide le montant, met à jour l'en-tête et l'historique sans recharger la page, et confirme dans la snackbar.
4. **Historique** (`partials/asset_history.html`) : toutes les valorisations, de la plus récente à la plus ancienne. Chacune montre sa date, sa valeur et l'écart avec la précédente (« +2 450,00 € », en `error` s'il est négatif, « inchangée » s'il est nul). Pour un prêt, un remboursement donne un écart positif : la dette baisse, le patrimoine monte. Vide : « Aucune valorisation pour l'instant : saisis la première ci-dessus ».
   - Chaque ligne a un bouton texte « Supprimer » (`btn-text-error`, nom accessible « Supprimer la valorisation du … ») pour corriger une saisie erronée, avec une confirmation qui cite la date et le montant. Un refus s'affiche en haut de l'historique ; un succès met à jour l'en-tête, l'historique et les sections de gestion sans recharger la page.
5. **Modifier le nom ou le type** (`partials/asset_manage.html`) : section repliée (`<details>`, secondaire). Quand plusieurs devises sont proposées, la devise s'y ajoute (« Modifier le nom, le type ou la devise ») ; une devise figée s'affiche alors en texte, avec la raison. Pour un compte bancaire, la banque (liste) et le numéro s'y modifient aussi (vides : non renseignés). Après un succès, la page se recharge avec la confirmation.
6. **Supprimer** : ce qui sera perdu, chiffré et accordé (« son historique : 3 valorisations », « aucune valorisation »), et bouton `btn-outlined-error`, avec confirmation par `hx-confirm` (voir [Web](../tech/web.md#htmx)). Après un succès, retour à la page Actifs, avec la confirmation dans la snackbar.

En-tête, historique et sections de gestion se rechargent ensemble après une valorisation : le nombre de valorisations annoncé et le verrou de la devise restent justes sans recharger la page.

## Opérations d'un compte (`transactions_page.html`)

Page d'un compte bancaire, accessible depuis la page de l'actif ; l'onglet « Actifs » reste actif. De haut en bas :

1. **Retour** « ← nom du compte ».
2. **Importer un relevé** : le relevé (CSV ou PDF), le solde après la dernière opération (facultatif) et le bouton plein « Importer ». Pendant l'envoi, le bouton est désactivé (htmx). Un succès recharge la page avec la confirmation dans la snackbar (« 2 opérations importées. Solde de 1 300,00 € enregistré. ») ; un refus s'affiche sous le formulaire, ou en haut de la page sans JavaScript. Règles : [Transactions](../specs/transactions.md).
3. **En attente** (seulement s'il y en a) : ce que la banque annonce sans l'avoir comptabilisé, avec leur somme en titre et une phrase qui dit qu'elles ne comptent pas dans le budget. Informatif : pas de catégorie à choisir.
4. **Liste** : le nombre d'opérations en titre, puis une ligne par opération (date, libellé, montant en `error` si c'est une dépense), de la plus récente à la plus ancienne. Vide : « Aucune opération pour l'instant : importe ton premier relevé ci-dessus ».

## Catégories (`categories_page.html`)

Page du menu Réglages. De haut en bas :

1. **Titre**, puis un message d'erreur éventuel (refus d'un formulaire, ou d'une suppression).
2. Sans aucune catégorie : **Ajouter les catégories usuelles** (bouton plein), qui explique à quoi servent les catégories. Sinon, la **liste** : chaque catégorie avec son sens et un bouton texte « Supprimer » (confirmation qui annonce que les opérations redeviennent sans catégorie), et dessous ses règles (« Libellé contenant « carrefour » », avec leur propre bouton « Supprimer »).
3. **Nouvelle règle** (une fois des catégories créées) : le motif, la catégorie, le bouton plein « Ajouter la règle ». Dessous, le bouton tonal **Classer les opérations**, avec sa phrase d'explication : il applique les règles aux opérations sans catégorie, sans toucher à celles classées à la main.
4. **Nouvelle catégorie** : le nom, le sens, le bouton plein « Ajouter ».

Sur la page des opérations d'un compte, chaque ligne porte désormais, sous la date, le libellé et le montant, une liste de catégories (présélectionnée) et un bouton tonal « Classer ». Sans catégorie créée, la page renvoie vers la page Catégories.

## Budget (`budget_page.html`)

Un mois à la fois, dans l'adresse. De haut en bas :

1. **Navigation de mois** : « ← Mois précédent » et, sauf pour le mois en cours, « Mois suivant → » (boutons texte, sans JavaScript), puis le titre « Budget · octobre 2026 ».
2. **Ce qu'il reste du mois** (`xl-increased`) : le résultat en grand (`display-medium`, `error` s'il est négatif), puis les revenus et les dépenses. Si des opérations n'ont pas de catégorie, une phrase le dit (couleur `tertiary`) et renvoie vers les comptes.
3. **Enveloppes** : une ligne par catégorie avec « dépensé sur plafond », la jauge (`meter`, `tertiary` quand l'enveloppe est proche, `error` quand elle est dépassée), l'état écrit (« Reste 120,00 € », « Dépassé de 50,00 € », « · proche du plafond ») et un bouton texte « Retirer » avec confirmation. Vide : « Aucune enveloppe ce mois-là ».
4. **Fixer un plafond** : la catégorie de dépense, le plafond, le bouton plein « Fixer ». Il précise que le plafond vaut à partir de ce mois.
5. **Épargne** (`xl-increased`) : la suggestion en titre (« Verse 500,00 € sur Livret A » ou « Rien à verser ») et son explication, les soldes (comptes courants, livret), le **matelas** et d'où il vient (« évalué : le mois le plus cher des 3 derniers mois complets… » ou « fixé par toi »), puis le rappel que worthfold ne déplace pas d'argent. Le réglage (livret, plafond, matelas facultatif) est dans une section repliée, ouverte tant que rien n'est réglé. Règles : [Budget et épargne](../specs/budget.md).

## Paiements (`payments_page.html`)

Page du menu Réglages. De haut en bas :

1. **Titre**, puis un message d'erreur éventuel.
2. **Repéré dans tes relevés** (seulement s'il y a des suggestions, et alors en premier : c'est le chemin le plus court) : le nom, le sens, le compte, le rythme, le montant et le nombre d'opérations qui l'ont fait repérer, avec un bouton tonal « Ajouter ».
3. **Échéances** : le calendrier, une ligne par échéance (date, nom, état, montant), précédé d'une phrase qui dit ce qu'il montre. L'état est écrit : « Prévu », « Constaté : 700,00 € » (en `primary`), « Manquant : rien n'est arrivé » (en `error`), « Sautée » (barrée) ; un montant estimé s'affiche « ≈ 1 850,00 € ± 150,00 € ». Une échéance non constatée a une section repliée « Corriger cette échéance » : Sauter, Fixer un autre montant, Rétablir.
4. **Mes paiements** : chaque paiement décrit en toutes lettres (« Tous les 3 mois, le 10, à partir de février 2026 », « Observé d'après tes relevés… », « révision estimée de 2 % chaque 15 octobre »), une section repliée « Nouveau montant » (palier : date, montant, marge) et un bouton texte « Supprimer » avec confirmation.
5. **Ajouter un paiement** : un formulaire en blocs (*Quoi*, *Quand*, *Combien*), ouvert tant qu'il n'y a aucun paiement. Le chemin courant (un paiement mensuel d'un montant fixe) ne montre que les champs de base. Ce qui est rare est replié : « Précisions selon le rythme choisi » (tous les N mois, certains mois, une seule fois), « Montant variable ou révisé chaque année » et « Limiter dans le temps ». Les zones repliées sont dans le formulaire : elles sont envoyées même fermées, et la page n'a pas besoin de JavaScript. Règles : [Paiements prévus](../specs/payments.md).

## Prévisions (`forecast_page.html`)

Page en lecture seule. De haut en bas :

1. **Solde d'aujourd'hui** des comptes courants.
2. **Alertes** (`bg-error-container`, `role="alert"`) : découvert attendu ou possible, passage sous le matelas.
3. **Trois horizons** (3, 6 et 12 mois) : solde attendu, fourchette prudent–favorable, patrimoine net.
4. **Évolution prévue** : la courbe du solde attendu (trait plein) et la fourchette du prudent au favorable (pointillés et bande), du jour à la fin de la période. C'est le même SVG que le graphique du tableau de bord (`chart.rs`) ; chaque point donne sa date, son solde et sa fourchette au survol.
5. **Sur quoi ça repose** : les hypothèses en phrases, juste sous la courbe, car elles conditionnent la lecture des chiffres.
6. **Détail mois après mois**, zone repliée : solde attendu, mouvement du mois, fourchette ; « sous le matelas » et « à découvert » sont dits en toutes lettres, pas par la seule couleur. Règles : [Prévisions](../specs/forecast.md).

## Connexion bancaire (`bank_page.html`, `/bank`)

1. Sans configuration, une carte explique les variables à renseigner ; le reste de la page est masqué.
2. **Une carte par banque connectée** : les jours de consentement restants (en `text-tertiary` à 14 jours ou moins, en `text-error` une fois expiré, toujours dits en toutes lettres), puis ses comptes, chacun avec un sélecteur « Reçoit les opérations dans » et sa date de dernière lecture, et « Retirer la connexion » (confirmation htmx).
3. **Lire les opérations**, visible dès qu'un compte est rattaché : « Lire maintenant » (bouton plein) et « Relire les 90 derniers jours » (tonal). Pendant la lecture, le bouton est désactivé et « Lecture en cours… » s'affiche ; le résultat revient dans la snackbar, un échec sous la carte.
4. **Connecter ou renouveler une banque** : nom et pays. Le champ « Banque » propose les banques du pays saisi (`datalist` rechargée par htmx quand le pays change) ; on peut aussi taper un nom, et sans JavaScript c'est le seul moyen. Règles : [Connexion bancaire](../specs/bank.md).

## Connexion, première configuration et compte

Les pages **Connexion** et **Première configuration** n'ont pas de navigation (on n'est pas encore entré) : une carte centrée (`rounded-xl-increased`, `max-w-md`), le nom « worthfold », un titre, les champs et un bouton plein.

- **Connexion** : un champ « Mot de passe » (focus à l'ouverture) et « Se connecter ». Un refus s'affiche au-dessus du formulaire (`bg-error-container`, `role="alert"`) ; il ne dit jamais ce qui manque (« Mot de passe incorrect. »). Après trop d'essais, il dit combien d'attente.
- **Première configuration** (seulement tant qu'aucun mot de passe n'existe) : le code de configuration affiché dans la console, le mot de passe et sa confirmation, avec l'exigence de longueur écrite. Une fois enregistré, l'utilisateur est connecté.
- **Banques** (menu Réglages, `/banks`) : la liste des banques, chacune avec son nombre de comptes et son format CSV en une ligne, repliée dans un `details` « Modifier » (les réglages, « Enregistrer les réglages », « Supprimer la banque » avec confirmation htmx). Un encadré « Relevé PDF (facultatif) » règle la lecture des PDF. Dessous, « Nouvelle banque » : le nom et les mêmes réglages, préremplis avec le format le plus courant. Règles : [Banques et format CSV](../specs/banks.md).
- **Compte** (menu Réglages) : « Changer le mot de passe » (actuel, nouveau, confirmation) et « Se déconnecter ». Le changement ferme les autres sessions, et le dit.

Tous ces formulaires sont des envois classiques (sans htmx) : une redirection, et la confirmation éventuelle dans la snackbar.

## Graphique

Le SVG (`crates/server/src/chart.rs`) est intégré à la page par htmx et utilise les mêmes jetons (sans JavaScript, il est servi seul et charge `/static/app.css`) :

- courbe en escalier et aire en `primary` (aire à 12 %), points en `primary` entourés de la couleur de la carte ;
- grille en `outline-variant`, ligne du zéro en `outline` pointillé ;
- graduations et dates en `on-surface-variant`, police Roboto Flex, chiffres tabulaires ;
- au survol, un point grossit avec le ressort `spatial-fast`.

Fond transparent : la carte qui l'accueille fournit la surface. Le SVG déclare `color-scheme: light dark`, sans quoi, servi seul, le navigateur lui donnerait un fond clair opaque en thème sombre.

Lisibilité sur téléphone : le graphique est dessiné sur 720 unités de large et réduit à environ 330 px. Une media query de ses styles (écran ≤ 560 px) agrandit graduations, dates et points pour garder environ 11 px affichés, et les traits ont une épaisseur constante (`vector-effect: non-scaling-stroke`).

Servi seul, le SVG est du XML : aucun `<` ni `&` dans ses styles, même en commentaire, sinon tout le document est rejeté (un test le vérifie). Intégré, ses styles deviennent globaux : ses classes sont préfixées `chart-`.
