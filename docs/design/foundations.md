# Design system : fondations

worthfold suit **Material 3 Expressive**, sans en utiliser les composants : pas de bibliothèque de composants ni de composant web JavaScript, seulement du HTML rendu par askama, des classes **Tailwind CSS v4** compilées au build, et htmx pour les mises à jour partielles. Material fournit les règles (rôles de couleur, échelles, états, accessibilité) et les valeurs officielles ; la marque fournit la couleur source. Application aux écrans : [Écrans](screens.md).

Principes :

- **Minimalisme** : un élément ne s'ajoute que s'il sert la lecture ou l'action.
- **Jetons uniquement** : couleurs, tailles de texte, arrondis et courbes de mouvement viennent des jetons ci-dessous, déclarés dans `crates/server/styles/app.css` (`@theme`). Les palettes et échelles par défaut de Tailwind sont désactivées : `bg-red-600`, `text-sm` ou `rounded-2xl` de Tailwind n'existent pas ici, et jamais de valeur arbitraire (`text-[13px]`, `rounded-[20px]`), sauf un rapport d'aspect.
- **Accessible par défaut** : contrastes WCAG AA, focus toujours visible, cibles de 48 px.
- **Le style sans JavaScript** : Tailwind est un exécutable autonome qui produit une feuille de style au build ; même les ressorts Expressive sont en CSS. Le seul JavaScript de la page est htmx, pour les échanges avec le serveur.

## Sources des valeurs

| Domaine | Source |
|---|---|
| Couleurs | Material Color Utilities (port Rust `material-colors`), schéma *Tonal spot*, spécification 2025, couleur source `#0F766E` |
| Formes | `ShapeTokens.kt` de Jetpack Compose Material 3 (androidx) |
| Typographie | `TypeScaleTokens.kt` (androidx) ; police Roboto Flex, police par défaut de M3 Expressive |
| Mouvement | `ExpressiveMotionTokens.kt` (androidx) : ressorts convertis en courbes CSS `linear()` |

Pour changer la couleur de la marque : régénérer les deux schémas (clair et sombre) depuis la nouvelle couleur source avec `material-colors`, remplacer les valeurs dans `app.css`, puis revérifier les contrastes ci-dessous.

## Couleurs

On choisit une couleur par son **rôle**, jamais par son apparence. Utilitaires Tailwind : `bg-<rôle>`, `text-<rôle>`, `border-<rôle>`.

| Rôle | Clair | Sombre | Usage |
|---|---|---|---|
| `primary` | `#1d6a63` | `#9cd1c9` | Bouton principal, liens, courbe du graphique, nom de l'appli |
| `on-primary` | `#e2fffa` | `#0f4843` | Texte sur `primary` |
| `primary-container` | `#a8f0e7` | `#265b55` | Héros du patrimoine net |
| `on-primary-container` | `#045c56` | `#b8eee6` | Texte sur `primary-container` |
| `secondary` | `#4a6460` | `#b1ccc8` | Indicateur de focus |
| `secondary-container` | `#cce8e4` | `#27403d` | Bouton tonal (action secondaire) |
| `on-secondary-container` | `#3d5653` | `#a9c5c0` | Texte sur `secondary-container` |
| `error` | `#a83836` | `#fa746f` | Montants négatifs, action destructive |
| `error-container` | `#fa746f` | `#871f21` | Bandeaux d'erreur, héros d'un patrimoine négatif |
| `on-error-container` | `#6e0a12` | `#ff9993` | Texte sur `error-container` |
| `surface` | `#f6faf8` | `#0a0f0e` | Fond de la page |
| `surface-container-low` | `#eef5f3` | `#0e1514` | Cartes (graphique, liste, formulaires) |
| `on-surface` | `#2a3433` | `#dce8e5` | Texte courant |
| `on-surface-variant` | `#56615f` | `#a2adab` | Texte discret : types, dates, libellés, graduations |
| `outline` | `#727d7b` | `#6d7876` | Contour des champs, ligne du zéro |
| `outline-variant` | `#a9b4b2` | `#3f4a48` | Séparateurs, grille du graphique (décoratif) |
| `tertiary` | `#356579` | `#d9f2ff` | Signalements discrets : « à mettre à jour », « jamais valorisé » |
| `inverse-surface` / `inverse-on-surface` | `#0a0f0e` / `#999e9c` | `#f6faf8` / `#515655` | Snackbar |

Les autres rôles `tertiary` et niveaux de `surface-container` sont déclarés pour la suite mais pas encore utilisés.

Le thème sombre suit la préférence système (`prefers-color-scheme`) : les mêmes variables changent de valeur, les classes ne changent pas.

### Contrastes mesurés

| Combinaison | Clair | Sombre | Minimum |
|---|---|---|---|
| Texte (`on-surface`) sur carte | 11,6:1 | 14,7:1 | 4,5:1 |
| Texte discret (`on-surface-variant`) sur carte | 5,8:1 | 8,0:1 | 4,5:1 |
| Héros (`on-primary-container` / `primary-container`) | 6,1:1 | 6,1:1 | 4,5:1 |
| Héros négatif, bandeau d'erreur (`on-error-container` / `error-container`) | 4,5:1 | 4,6:1 | 4,5:1 |
| Montant négatif, bouton de suppression (`error`) sur carte | 5,8:1 | 6,9:1 | 4,5:1 |
| Bouton plein (`on-primary` / `primary`) | 6,0:1 | 6,1:1 | 4,5:1 |
| Bouton tonal (`on-secondary-container` / `secondary-container`) | 6,1:1 | 6,1:1 | 4,5:1 |
| Lien (`primary`) sur page / sur carte | 6,1:1 / 5,8:1 | 11,4:1 / 10,9:1 | 4,5:1 |
| Contour de champ (`outline`) sur carte | 3,8:1 | 4,0:1 | 3:1 |
| Focus (`secondary`) sur carte | 5,8:1 | 10,9:1 | 3:1 |
| Signalement (`tertiary`) sur carte | 5,8:1 | 15,9:1 | 4,5:1 |
| Snackbar (`inverse-on-surface` / `inverse-surface`) | 7,1:1 | 7,1:1 | 4,5:1 |

Règle : du texte atteint 4,5:1 ; le contour d'un contrôle (champ, bouton sans fond) et l'indicateur de focus atteignent 3:1. `error-container` est au minimum : n'y poser que `on-error-container`, jamais de texte discret.

## Typographie

Police : **Roboto Flex**, variable (graisses 100 à 1000, taille optique automatique), sous-ensemble latin de 85 Ko embarqué dans le binaire et servi par `/static/fonts/roboto-flex-latin.woff2` : aucun appel à Google Fonts. Licence SIL OFL 1.1 (`crates/server/assets/fonts/OFL.txt`). Le sous-ensemble couvre l'espace fine insécable, `€` et le signe moins `−`.

| Rôle M3 | Classe | Taille / interligne | Usage |
|---|---|---|---|
| Display large | `text-display-large` | 57 / 64 px | Patrimoine net, sur ordinateur |
| Display medium | `text-display-medium` | 45 / 52 px | Patrimoine net, sur mobile |
| Headline small | `text-headline-small` | 24 / 32 px | Titre d'une page secondaire |
| Title large | `text-title-large` | 22 / 28 px | Nom de l'application |
| Title medium | `text-title-medium` | 16 / 24 px, 500 | Titres de carte, nom d'un actif, montants de la liste |
| Body large | `text-body-large` | 16 / 24 px | Texte courant, champs |
| Body medium | `text-body-medium` | 14 / 20 px | Texte secondaire, messages |
| Label large | `text-label-large` | 14 / 20 px, 500 | Boutons, surtitres |
| Label medium | `text-label-medium` | 12 / 16 px, 500 | Libellés de champ, dates |

Chaque classe porte la taille, l'interligne, l'approche et, s'il y a lieu, la graisse du rôle. **Emphase Expressive** : une graisse au-dessus du rôle (`font-medium` sur un display, `font-semibold` sur le nom de l'appli), jamais une taille arbitraire. Les montants utilisent `tabular-nums` pour s'aligner. 12 px est la taille minimale.

## Forme

Échelle M3 Expressive. Utilitaires `rounded-<jeton>`.

| Jeton | Rayon | Usage |
|---|---|---|
| `xs` | 4 px | Champs de formulaire (« outlined ») |
| `sm` | 8 px | — |
| `md` | 12 px | Bouton pendant l'appui (morphing) |
| `lg` | 16 px | Bandeaux d'erreur |
| `lg-increased` | 20 px | — |
| `xl` | 28 px | Cartes |
| `xl-increased` | 32 px | Grandes surfaces mises en avant : héros, carte principale d'une page |
| `2xl` | 48 px | — |
| `full` | pilule | Boutons au repos |

Le contraste des formes fait partie du langage Expressive : grandes surfaces très arrondies, champs presque droits, boutons en pilule.

## Élévation

L'interface est plate : la hiérarchie vient des surfaces (`surface` → `surface-container-low` → `primary-container`), pas des ombres. Aucune ombre n'est utilisée pour l'instant.

## Mouvement

Les ressorts officiels de M3 Expressive sont convertis en courbes CSS `linear()` : le dépassement est réel, sans JavaScript.

| Jeton (`ease-*`) | Ressort (amortissement, raideur) | Durée | Usage |
|---|---|---|---|
| `spatial-fast` | 0,6 ; 800 | 360 ms | Petits éléments : morphing des boutons, points du graphique |
| `spatial` | 0,8 ; 380 (défaut) ou 200 (lent) | 435 ms / 600 ms | Déplacements, changements de taille |
| `effects` | 1,0 ; 3800 / 1600 / 800 | 150 / 230 / 330 ms | Couleurs et opacités (sans dépassement) |

- La courbe d'un ressort ne dépend que de son amortissement ; sa raideur donne la durée.
- **Morphing de forme** : un bouton passe de la pilule (`full`) à `md` pendant l'appui, avec `spatial-fast`.
- La préférence système de réduction des mouvements ramène toutes les transitions à une durée nulle (règle de base dans `app.css`). Exception : la snackbar apparaît et disparaît d'un coup, au lieu de disparaître immédiatement.

## États d'interaction

| État | Règle |
|---|---|
| Survol | Couche d'état (`state-layer`) : voile de la couleur du contenu à 8 %. Lien : `primary` et souligné |
| Focus clavier | Contour de 3 px en `secondary`, décalé de 2 px, sur tout élément (`:focus-visible`). Un champ montre son focus par un contour `primary` de 2 px |
| Appui | Couche d'état à 10 %, et morphing de la forme pour un bouton |
| Erreur | Bandeau `error-container` avec `role="alert"`, sous le formulaire concerné |
| Chargement | Squelette `animate-pulse` en `surface-container`, à la forme exacte du contenu attendu |

## Composants

Déclarés avec `@utility` dans `app.css`, à utiliser tels quels :

| Classe | Composant M3 | Usage |
|---|---|---|
| `btn-filled` | Bouton plein | L'action principale d'un formulaire |
| `btn-tonal` | Bouton tonal | Action fréquente mais secondaire (valoriser un actif) |
| `btn-outlined-error` | Bouton à contour, erreur | Action destructive |
| `btn-text-error` | Bouton texte, erreur | Action destructive secondaire, répétée dans une liste (supprimer une valorisation) |
| `field` | Champ « outlined » | Champs texte, dates et listes déroulantes |
| `field-label` | Libellé de champ | Enveloppe un champ et son libellé |
| `state-layer` | Couche d'état | Tout élément cliquable sans composant dédié |
| `snackbar` | Snackbar | Confirmation d'une action, qui disparaît seule (voir [UX](ux.md), règle 2) |

Écart assumé : le libellé d'un champ est posé au-dessus, pas flottant dans le contour comme dans M3 (le libellé flottant demande du JavaScript ou des contorsions CSS fragiles).

## Accessibilité

- Contrastes : voir le tableau ci-dessus.
- Cibles : 48 px de haut pour les boutons et les champs (`min-h-12`).
- Chaque champ a un libellé, visible ou réservé aux lecteurs d'écran (`sr-only`) quand le contexte le rend évident à l'œil.
- Les messages d'erreur portent `role="alert"`. La langue de la page est `fr`.

## Espacement et mise en page

- Grille de 4 px de Tailwind. Espacement entre cartes : `gap-4` ; marges intérieures : `p-4 md:p-6`, `p-6 md:p-10` pour les grandes surfaces.
- Une colonne centrée, `max-w-3xl`, marges latérales `px-4`.
- Deux tailles d'écran : mobile (empilement) et à partir de 768 px (`md:`, colonnes).
