# Règles d'UX

Dix règles simples, à vérifier sur chaque écran et chaque nouvelle fonctionnalité. Elles complètent les [fondations](foundations.md) (le « à quoi ça ressemble ») par le « comment ça se comporte ». Application aux écrans : [Écrans](screens.md).

## 1. Une action principale par zone

Chaque carte a au plus une action mise en avant ; les autres s'effacent.

- Bouton plein (`btn-filled`) : l'action principale (Ajouter, Enregistrer).
- Bouton tonal (`btn-tonal`) : une action fréquente mais secondaire (Valoriser).
- Bouton à contour rouge (`btn-outlined-error`) : une action destructive, toujours à l'écart des autres.

## 2. Chaque action a un retour visible

L'utilisateur sait toujours ce qui se passe, dans les trois temps d'une action :

- **Pendant** : le bouton est désactivé et le curseur indique l'attente (`hx-disabled-elt`) ; un double clic n'envoie pas deux fois.
- **Après un succès** : un message bref le confirme (snackbar M3, en bas de l'écran, qui disparaît seule) et ce qui a changé se met à jour sous les yeux. Si l'action change de page, la confirmation la suit (message flash).
- **Après un refus** : le message apparaît sous le formulaire concerné, pas ailleurs.

## 3. Prévenir plutôt que corriger

- Des valeurs par défaut utiles : la date du jour, la devise la plus courante.
- Les contraintes sont dans les champs (`required`, `maxlength`, `max` sur une date, clavier numérique avec `inputmode="decimal"`).
- Le serveur applique **les mêmes règles** : un champ du navigateur se contourne, une règle métier non.
- Une saisie libre est acceptée sous toutes ses formes raisonnables (`1 234,56`, `1234.56`).

## 4. Des messages dans les mots de l'utilisateur

Un message dit **ce qui ne va pas et comment corriger**, en français courant.

- « Saisis un nombre, par exemple 1 234,56 », plutôt que « Format invalide ».
- Jamais d'identifiant technique, de code d'erreur ou de détail d'une panne : une panne affiche un message générique, le détail va dans les journaux.
- Le tutoiement, comme dans le reste de l'interface.

## 5. Ne jamais perdre une saisie

Après un refus, ce qui a été tapé reste en place, pour corriger sans tout ressaisir. C'est l'une des raisons d'htmx (voir [Web](../tech/web.md#formulaires)).

**Limite assumée sans JavaScript** : un refus redirige vers la page avec le message (`?error=`), et le formulaire revient vide. Le mode sans JavaScript est un repli : l'essentiel est que chaque action y fonctionne, pas qu'il offre le même confort.

## 6. Les actions destructives se confirment et annoncent leurs conséquences

Avant une suppression : ce qui sera perdu, chiffré et accordé (« et ses 3 valorisations », jamais « valorisation(s) »), et le fait que c'est définitif. Le bouton est séparé des actions courantes.

## 7. L'essentiel d'abord, le détail à la demande

Ce qu'on consulte tout le temps est visible ; ce qu'on fait de temps en temps se révèle d'un geste. Sur téléphone surtout, chaque élément affiché en permanence doit le mériter.

## 8. Montrer la fraîcheur des données

Une valeur de patrimoine n'a de sens qu'avec sa date. Chaque valeur affiche la date de sa dernière mise à jour, et une valeur ancienne (plus de 90 jours) ou absente est signalée, pour savoir quoi mettre à jour.

## 9. Un état vide guide vers la première action

Une liste ou un graphique vide explique pourquoi il est vide et ce qu'il faut faire pour le remplir, plutôt que de laisser un trou.

## 10. Cohérence et accessibilité

Les mêmes gestes produisent les mêmes effets partout (un formulaire se comporte toujours de la même façon), et les règles d'accessibilité des [fondations](foundations.md#accessibilité) s'appliquent : contrastes, focus visible, cibles de 48 px, libellés, `role="alert"` et `role="status"`.

## Audit de l'interface (9 novembre 2026)

Relevé fait avec ces règles, puis corrigé :

- [x] **Règle 2** : aucun retour pendant l'envoi (double clic possible) → bouton désactivé pendant la requête, curseur d'attente.
- [x] **Règle 2** : un succès ne se confirmait que par le changement des chiffres → snackbar (« Valorisation de 3 300,00 € enregistrée »), annoncée aux lecteurs d'écran (`role="status"`).
- [x] **Règle 3** : une date future était acceptée puis ignorée par le total, sans explication → `max` sur le champ date et refus côté serveur.
- [x] **Règle 4** : « L'actif 01a1c0d5-… n'existe pas » montrait un identifiant technique → « Cet actif n'existe plus : il a peut-être été supprimé ».
- [x] **Règle 4** : « Le montant « abc » n'est pas un nombre » ne disait pas comment corriger → ajout d'un exemple de saisie.
- [x] **Règle 7** : sur téléphone, un formulaire de valorisation complet sous chaque actif → replié derrière un bouton « Valoriser » sur petit écran, toujours visible sur ordinateur.
- [x] **Règle 8** : rien ne signalait une valeur ancienne → mention « à mettre à jour » au-delà de 90 jours, « jamais valorisé » sans valeur.
- [x] **Règle 9** : liste et graphique vides guidaient déjà vers l'ajout et la première valorisation (conforme).
- [x] **Règle 2** : après une modification ou une suppression, le retour à la liste se faisait sans confirmation → message flash affiché dans la snackbar (« « Maison » supprimé »), avec ou sans JavaScript.
- [x] **Règle 6** : « et ses 0 valorisation(s) » → nombre accordé (« aucune valorisation », « sa valorisation », « ses 3 valorisations »), sans « (s) ».
- [x] **Règle 2** : les envois sans JavaScript n'avaient aucune confirmation → même message flash.
- [x] **Règle 7** : une seule page mêlait consultation et gestion, la liste et ses formulaires en occupant les deux tiers → un tableau de bord sans formulaire (total, graphique, carte de synthèse) et une page Actifs pour gérer, reliés par une navigation à deux destinations.
- [x] **Règle 8** : les actifs à mettre à jour n'étaient visibles qu'en parcourant la liste → listés sur le tableau de bord, chacun menant à sa ligne.
- [x] **Règles 1 et 7** : la valorisation se faisait dans la liste, qui présentait et servait à saisir à la fois → une page par actif, où la valorisation est l'action principale, juste sous la valeur ; la liste ne fait plus que présenter, chaque ligne menant à l'actif.
- [x] **Règle 8** : l'historique d'un actif n'était visible nulle part → historique complet sur la page de l'actif, avec l'écart entre deux valeurs.
- [x] **Règle 2** : après un succès, une zone d'erreur vide restait affichée (réponse avec des espaces) → réponse rognée.
- [x] **Règle 6** : après une valorisation sans rechargement, la confirmation de suppression annonçait un nombre de valorisations périmé → les sections de gestion se rechargent avec l'en-tête et l'historique.
- [x] **Règle 3** : une valorisation saisie par erreur ne pouvait pas être retirée (seulement masquée par une nouvelle saisie) → suppression depuis l'historique, avec une confirmation qui cite la date et le montant.
- [x] **Règles 1, 5, 10** : conformes (vérifié sur le tableau de bord, la page Actifs et la page d'un actif).
