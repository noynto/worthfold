# Connexion bancaire

> **Reportée** : pour le moment, l'application se concentre sur l'import de relevés CSV ([Banques et format CSV](banks.md)). Cette connexion reste en place mais n'est plus la voie principale.

worthfold peut lire les opérations et le solde de tes comptes directement chez ta banque, par l'agrégateur **EnableBanking** (accès aux comptes, lecture seule). Les opérations lues deviennent des [transactions](transactions.md) ordinaires : classées par les mêmes règles, comptées dans le budget et le prévisionnel. L'import CSV reste disponible, et les deux peuvent se compléter. Le port `BankAggregator` (crate `bank`) isole l'agrégateur : EnableBanking n'est qu'un adaptateur (`infrastructure::enablebanking`).

## Configuration

| Variable | Rôle |
|---|---|
| `WORTHFOLD_ENABLEBANKING_APP_ID` | L'identifiant (UUID) de l'application enregistrée chez EnableBanking |
| `WORTHFOLD_ENABLEBANKING_KEY_PATH` | Le fichier PEM de sa clé privée RSA |
| `WORTHFOLD_BANK_REDIRECT_URL` | Où la banque renvoie l'utilisateur. Par défaut `<WORTHFOLD_PUBLIC_URL>/bank/callback` si l'adresse publique est définie, sinon `http://localhost:<port>/bank/callback` ; elle doit figurer parmi les adresses déclarées chez EnableBanking |

Les deux premières vont ensemble (une seule des deux est une erreur de démarrage) ; sans elles, la page Banque explique ce qui manque et le reste de l'application fonctionne. Une clé illisible arrête le démarrage.

**Le secret, c'est la clé privée.** Chaque appel est signé par un JWT RS256 (`kid` = identifiant de l'application, durée de vie 5 minutes). Le fichier reste hors de Git (`/secrets/` et `*.pem` sont ignorés), avec les permissions 600. L'identifiant de session enregistré en base ne permet aucun appel sans cette clé : rien n'est chiffré en base, et aucun coffre n'est nécessaire pour une application locale à un seul utilisateur.

## Le parcours

1. **Connecter** : on indique la banque (son nom chez EnableBanking) et le pays. worthfold crée une demande (un état aléatoire, à usage unique) et envoie l'utilisateur chez sa banque, pour un consentement de **90 jours**.
2. **Retour** (`/bank/callback`) : la banque renvoie un code et l'état (le cookie de session accompagne cette navigation : `SameSite=Lax`, voir [Sécurité](../tech/security.md#authentification)). Un état inconnu ou déjà utilisé est refusé. Le code est échangé contre une session, et ses comptes sont enregistrés. Un refus ou une erreur de la banque ne crée rien, et son texte n'est jamais affiché.
3. **Rattacher** chaque compte de la banque à un compte bancaire de worthfold (même devise ; un compte de worthfold ne reçoit les opérations que d'un seul compte de la banque).
4. **Lire** : bouton « Lire maintenant » ou tâche planifiée.

## La lecture

- Première lecture d'un compte : les 90 derniers jours. Ensuite, depuis la dernière lecture, **avec 7 jours de recouvrement** : une opération qui devient « comptabilisée » après coup n'est pas ratée.
- **Relecture complète** : le bouton « Relire les 90 derniers jours » ignore la dernière lecture et remonte à 90 jours. Il rattrape une opération apparue chez la banque avec une date plus ancienne que le recouvrement. Les doublons restent écartés par empreinte.
- Les opérations comptabilisées (`BOOK`) sont importées comme transactions. Le libellé est le motif du virement, sinon la contrepartie, sinon la note.
- Les doublons sont écartés par empreinte (date, montant, libellé), comme pour le CSV. Un même relevé importé en CSV **et** lu par la banque peut donc doubler une opération dont le libellé diffère : choisis une source par compte.
- **Opérations en attente** (`PDNG`, paiements par carte récents) : lues à chaque lecture, sur les 30 derniers jours, elles **remplacent** celles de la lecture précédente. Ce ne sont pas des transactions (ni historique ni catégorie) : elles peuvent changer de montant ou de libellé, ou disparaître, avant d'être comptabilisées. Elles apparaissent sur la page du compte (« En attente »), n'entrent pas dans le budget, et le prévisionnel les retranche du solde de départ. Une opération rendue à la fois en attente et comptabilisée n'est gardée que comme comptabilisée. Si leur lecture échoue, la liste est vidée (mieux vaut rien qu'un solde faux) sans faire échouer la lecture des opérations comptabilisées.
- Le solde comptabilisé devient une valorisation du compte, sauf s'il est identique à la dernière : il entre dans le patrimoine net.
- Un compte en échec n'arrête pas les autres ; la raison est dite, le détail technique est journalisé.
- Les banques limitent les lectures hors présence de l'utilisateur (souvent 4 par jour) : la tâche planifiée ne doit pas être plus fréquente.

## L'expiration du consentement

La page affiche les jours restants et prévient **14 jours avant**. Un consentement expiré n'est plus lu (la lecture le signale, et la tâche sort en code 1). **Renouveler** = connecter à nouveau la même banque : les comptes reconnus gardent leur rattachement et leur date de lecture, et l'ancien consentement est remplacé. Les transactions déjà lues ne sont jamais supprimées avec un consentement ; supprimer un compte de worthfold détache simplement ses comptes de la banque.

## La tâche planifiée

`WORTHFOLD_BANK_SYNC_TASK=true worthfold` lit tous les comptes rattachés et s'arrête (code 1 si un compte ou un consentement est en échec). Voir [Tâches](../tech/tasks.md).

## Limites

- Un seul agrégateur (EnableBanking), clients particuliers (`personal`).
- Le nom de la banque se tape ou se choisit parmi des suggestions : la liste des banques du pays vient d'EnableBanking (`GET /aspsps?country=…`) et n'est qu'une aide, un nom tapé à la main reste accepté. Sans JavaScript ou en cas de panne, le champ est libre.
- La connexion et le renouvellement demandent l'utilisateur : rien ne se renouvelle seul.
