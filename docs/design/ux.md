# Expérience utilisateur

Ce document dit **comment l'application se comporte** : pour qui, pour quoi faire, et selon quelles règles. L'apparence est décrite dans les [fondations](foundations.md) et les [écrans](screens.md) ; la protection des données dans [Sécurité](../tech/security.md).

## Pour qui, pour quoi faire

Une seule personne suit son propre patrimoine sur son ordinateur, quelques minutes à la fois, une fois par semaine ou par mois. Elle n'est pas comptable : elle veut savoir *où elle en est* et *où elle va*, pas tenir une comptabilité. Conséquences :

- Les chiffres importants se lisent d'un coup d'œil ; le détail se demande.
- Rien n'est demandé qui puisse se déduire : la date du jour, la devise, la catégorie par règle, les opérations par la banque.
- L'application ne décide jamais à la place de l'utilisateur : elle **suggère** (paiements détectés, versement d'épargne) et il accepte ou non.
- Une donnée n'est jamais plus fiable qu'elle ne le paraît : une valeur a une date, une prévision a une fourchette et ses hypothèses.

## Parcours principaux

| Parcours | Chemin | Rythme |
|---|---|---|
| Savoir où j'en suis | Tableau de bord : total, graphique, actifs à mettre à jour | À chaque visite |
| Mettre à jour un actif | Actifs → l'actif → Valoriser | Mensuel |
| Suivre le mois | Budget : synthèse, enveloppes, versement d'épargne suggéré | Hebdomadaire |
| Alimenter un compte courant | Banque (lecture) ou Opérations → Importer un relevé | Hebdomadaire ou mensuel |
| Déclarer ses échéances | Paiements : accepter les suggestions, ajouter un revenu ou une charge | À l'installation, puis rare |
| Anticiper | Prévisions : 3, 6 et 12 mois, alertes de découvert | Mensuel |
| Entrer | Connexion : un mot de passe ; à la première utilisation, le choix du mot de passe avec le code affiché dans la console | À chaque ouverture après 14 jours, ou après une déconnexion |
| Garder la banque connectée | Banque : renouveler avant l'expiration (90 jours) | Trimestriel |

Un parcours tient en peu de gestes et ne passe jamais par une page sans rapport. Un état vide mène à l'étape suivante du parcours (règle 9).

## Les règles

Douze règles simples, à vérifier sur chaque écran et chaque nouvelle fonctionnalité. Elles complètent les [fondations](foundations.md) (le « à quoi ça ressemble ») par le « comment ça se comporte ». Application aux écrans : [Écrans](screens.md).

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

- Des valeurs par défaut utiles : la date du jour. Un choix à une seule option n'est pas montré : la devise n'est pas demandée tant que seul l'euro est proposé.
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

## 11. Dire d'où vient une valeur

Une même information peut venir de plusieurs sources : saisie à la main, relevé importé, banque, calcul. L'utilisateur doit pouvoir distinguer ce qu'il a saisi de ce qui a été lu ou déduit, pour savoir quoi corriger et à qui se fier. Une suggestion se présente comme telle ; un montant prévu se distingue d'un montant constaté ; une prévision montre ses hypothèses.

## 12. Une action longue ou externe prévient et ne se répète pas

Une action qui attend un tiers (la banque, un fichier volumineux) montre qu'elle travaille, empêche le second envoi, et dit à la fin ce qu'elle a fait en chiffres (« 12 nouvelles opérations lues »). Si elle échoue en partie, elle dit quelle partie.

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

## Audit des écrans Opérations, Catégories, Budget, Paiements, Prévisions, Banque (26 novembre 2026)

Relevé fait avec les douze règles, sur les pages ajoutées depuis le premier audit. À corriger :

- [x] **Règles 2 et 12** : « Lire maintenant », « Relire les 90 derniers jours » et l'import d'un relevé étaient des envois classiques, sans retour pendant l'attente et avec double envoi possible → `hx-post` et `hx-disabled-elt` comme les autres formulaires, repli sans JavaScript ; la lecture bancaire affiche « Lecture en cours… » ; un refus s'affiche sous le formulaire.
- [x] **Règle 11** : la liste des opérations ne disait pas si une opération vient de la banque ou d'un relevé → mention discrète sous chaque libellé (« Lue chez la banque », « Importée d'un relevé »).
- [x] **Règle 3** : le nom de la banque se tapait à la main, et une faute donnait une erreur incompréhensible → suggestions des banques du pays (EnableBanking), le texte libre restant accepté.
- [x] **Règle 11** : sur Prévisions, « Sur quoi ça repose » était tout en bas de la page → remonté sous la courbe.
- [~] **Règle 10** : les pages ajoutées ont été regardées à 390 px de large (Paiements, Budget, Banque, Catégories, Actifs) : les colonnes se replient en une, rien ne déborde ; un défaut trouvé (texte des listes déroulantes collé en haut du champ, corrigé). Reste à mesurer au clavier et au lecteur d'écran.

Conformes : **règle 4** (messages de la banque, des paiements et des relevés en français courant, sans identifiant), **règle 6** (retirer une banque annonce que les opérations déjà lues restent), **règle 8** (la banque affiche la date de dernière lecture et les jours restants du consentement), **règle 9** (Budget, Paiements, Catégories et Prévisions guident depuis leur état vide).

## Évaluation de la charge cognitive (26 novembre 2026)

Estimée par lecture des templates (nombre de zones, de champs, de boutons, de notions), sans test avec un utilisateur. On distingue la charge **intrinsèque** (le sujet est compliqué) de la charge **superflue** (l'interface complique).

### Ce qui garde la charge basse

- **Hiérarchie typographique stable** : le texte courant et les titres de section gardent le même rôle d'une page à l'autre.
- **Une action pleine par zone** (règle 1) : 14 boutons pleins, 10 tonals, 7 destructifs, répartis en zones.
- **La couleur porte un sens** (erreur, avertissement, confirmation) et n'est jamais seule : le texte l'écrit aussi.
- **Le tableau de bord n'a aucun formulaire** : total, graphique, synthèse. C'est l'écran de chaque visite.
- **Rien n'est demandé qui se déduise** : date, devise, catégorie par règle, opérations par la banque.

### Où la charge monte

| Zone | Charge | Cause |
|---|---|---|
| Paiements | Forte, surtout intrinsèque | Échéances, suggestions, paiements, paliers, révisions, exceptions sur une seule page (245 lignes, 7 formulaires, 3 zones repliables) ; notions abstraites non expliquées avant d'être rencontrées |
| Navigation | Moyenne (corrigée) | Sept destinations au même niveau, dont de la configuration |
| Prévisions | Moyenne à forte | 40 à 50 chiffres (3 horizons, 3 scénarios, 13 mois) sans courbe pour les relier |
| Vocabulaire | Moyenne | Une quinzaine de termes à apprendre : enveloppe, matelas, versement suggéré, palier, scénario, consentement |
| Alimenter un compte | Moyenne | Deux chemins (banque, relevé) et des envois sans retour |
| Réglages liés | Moyenne | Catégories, règles, enveloppes et épargne sur trois pages |

### Pistes

- [x] **Navigation** : sept destinations ramenées à quatre, plus **Réglages** (Paiements, Banques, Connexion bancaire, Catégories, Compte), une page d'index. La navigation est adaptative : barre en bas sous 768 px, rail à gauche au-delà, avec des icônes et sans JavaScript.
- [x] **Paiements** : suggestions à accepter en premier, puis le calendrier et les paiements, chacun précédé d'une phrase qui dit ce qu'il montre ; dans le formulaire d'ajout, les champs propres à un rythme, le montant variable ou révisé et la limite dans le temps sont repliés (le chemin courant tient en sept champs). Paliers et exceptions restaient déjà repliés.
- [x] **Prévisions** : une courbe du scénario attendu avec la fourchette prudent–favorable (`chart::render_forecast`), le détail mois par mois derrière une zone repliable (règle 7).
- [x] **Prévisions** : « Sur quoi ça repose » remonté sous la courbe (règle 11).
- [x] **Retour pendant l'attente** sur la lecture bancaire et l'import (règles 2 et 12).
- [x] **Glossaire intégré** : une phrase sous le titre de chaque notion — enveloppe (Budget), matelas (Épargne), règle (Catégories), échéances et paiements (Paiements), mot du libellé (formulaire), scénarios (Prévisions) — plutôt qu'une page d'aide.
- [x] **Réglages liés** : Budget renvoie vers Catégories (« Les catégories et leurs règles se gèrent… ») et Catégories vers Budget (« Les plafonds par catégorie se fixent… »).

## Audit complémentaire après le rail de navigation (27 novembre 2026)

Pages regardées à 1100 et 390 px, connecté, sur l'application locale. Le mode sombre, le focus au clavier et les mises à jour htmx n'ont pas été observés.

- [x] **Phrase du Budget** : « opérations 68 sans catégorie » (le nombre était inséré après le premier mot) → « 68 opérations sans catégorie ».
- [x] **Repères de navigation** : le rail et la barre sont hors de `main`, avec un lien « Aller au contenu » visible au focus.
- [x] **Formulaire d'ajout d'un actif** : libellés en une ligne, aide sous le champ, deux colonnes égales (la banque et l'IBAN ne s'alignaient pas) ; même correction dans le formulaire de modification.
- [x] **Résultat du Budget** : tant que des opérations ne sont pas classées, le chiffre n'est plus en rouge et le titre dit « provisoire » (règle 11 : ne pas présenter une donnée incomplète comme une erreur).
- [x] Barre du bas : « Tableau de bord » renommé « Accueil » (il passait sur deux lignes) ; libellé actif en gras.
- [x] Réglages : séparateurs et chevrons, comme la liste des Actifs.
- [x] Paiements, sous 640 px : le sens et le compte passent sur leur ligne, le bouton « Ajouter » sous le texte.
- [x] Prévisions : « Patrimoine net » n'est affiché que s'il diffère du solde.
- [x] Banque non configurée : une phrase simple d'abord, les variables d'environnement dans « Comment l'activer » (repliable).

## Un formulaire de création par page (27 novembre 2026)

Pour garder une charge cognitive faible, la création d'une ressource a sa propre page, ouverte par un bouton plein sur la liste : Actifs (`/assets/new`), Paiements (`/payments/new`), Catégories (`/categories/new` et `/categories/rules/new`) et Banques (`/banks/new`). La liste ne sert qu'à lire ; la page de création a un seul objectif, « Annuler » et le retour à la liste. Un refus rouvre la page de création avec le message ; un succès renvoie vers la liste ou, pour un actif, vers sa page, où saisir sa première valeur.

On laisse en place les actions d'un geste (supprimer, ajouter une suggestion, classer les opérations), les modifications d'un élément déjà ouvert et les réglages à deux champs qui portent sur ce qu'on lit à côté : le plafond d'une enveloppe et le versement d'épargne, sur la page Budget, au mois qu'ils modifient.

### Limites

L'évaluation vient du code, pas d'une observation. Les comptes de formulaires et de champs sont ceux des gabarits : l'affichage réel dépend du nombre de paiements, de catégories ou de comptes. À confronter à un parcours réel avant de classer les pistes.
