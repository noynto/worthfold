# Patrimoine net et évolution

## Patrimoine net à une date

Le patrimoine net est la somme, pour chaque actif, de sa **dernière valorisation connue à cette date** (`service::net_worth`).

- Une valorisation postérieure à la date est ignorée.
- Un actif jamais valorisé compte pour zéro.
- Les prêts, enregistrés en négatif, viennent en déduction.
- Le tableau de bord affiche le patrimoine net du jour, décomposé en avoirs et dettes ; la page Actifs affiche chaque actif avec sa dernière valeur et sa date.

## Avoirs et dettes

`service::breakdown` sépare, à une date, la somme des dernières valeurs des **avoirs** et ce qui reste dû sur les **dettes** (les prêts), présenté en positif. Avoirs moins dettes donnent le patrimoine net. Un compte à découvert reste un avoir, de valeur négative.
- Si le total n'est pas calculable (devises mélangées), `ReadNetWorth` ne refuse pas : il rend la liste des actifs et un total en erreur (`Result<Money, CurrencyMismatch>`). Refuser toute la page empêcherait de corriger l'actif en cause.

## Évolution

L'historique (`service::net_worth_history`) calcule le patrimoine net **à chaque date où une valorisation a été saisie**, plus un dernier point à aujourd'hui pour prolonger la courbe.

- Entre deux saisies, le patrimoine vaut la dernière valeur connue : le graphique est donc **en escalier**. Une pente laisserait croire à une évolution progressive qui n'a pas eu lieu (un bien ajouté le 10 juin apparaît d'un coup ce jour-là).
- Un actif supprimé disparaît aussi de l'historique, avec ses valorisations.
- Sans aucune valorisation, le graphique affiche un message à la place de la courbe.

## Limites actuelles

- **L'euro, par choix** : le total et l'historique sont calculés en euros, et aucune conversion de devises n'est prévue. C'est aussi la seule devise proposée, ce qui garde le total toujours calculable. Si une autre devise est ajoutée un jour, un actif valorisé dans cette devise rendra le total indisponible : la page d'accueil affiche toujours la liste des actifs (pour pouvoir corriger), avec la raison à la place du total, et le graphique affiche le même message.
- **Prêts à jour à la main** : le capital restant dû n'est pas encore calculé à partir d'un tableau d'amortissement.
- **Pas d'authentification** : l'application n'écoute que sur la machine locale.
