# Sécurité

worthfold manipule des données financières personnelles et, depuis la connexion bancaire, détient un accès en lecture à de vrais comptes. Ce document dit ce qui est protégé, contre quoi, par quoi, et ce qui ne l'est pas encore. Les écarts connus sont listés en fin de document : ce sont des corrections à faire, pas des choix.

## Modèle de menace

Application personnelle, un seul utilisateur, lancée en local. Ce qu'on protège, et contre qui :

| Actif à protéger | Menace retenue | Hors périmètre (pour l'instant) |
|---|---|---|
| Les données (comptes, opérations, patrimoine) | Un site ouvert dans le même navigateur qui appelle le serveur local ; une autre machine du réseau ; un autre compte de la machine qui ouvre l'adresse locale | Un attaquant qui a déjà la main sur la machine ou sur la session de l'utilisateur |
| La clé privée EnableBanking | Fuite par le dépôt Git, les journaux, les droits de fichier | Vol de la clé par un programme qui tourne sous le même compte |
| Le consentement bancaire (`session_id`) | Détournement du retour de la banque (`/bank/callback`) ; fuite dans les journaux | |
| L'intégrité des données | Une modification déclenchée à l'insu de l'utilisateur (CSRF, clickjacking) | |

Un **mot de passe** protège l'accès (voir « Authentification ») : sans session valide, personne ne voit une page de données, même depuis la machine. Ce qui reste hors périmètre : un attaquant qui lit déjà le disque ou la mémoire du processus.

## Authentification

Un seul utilisateur, un mot de passe, des sessions. Le domaine est dans la crate `auth` ; les cas d'usage sont `SetupPassword`, `Login`, `Logout`, `CheckSession`, `ChangePassword` et `ResetPassword`.

- **Première configuration** : tant qu'aucun mot de passe n'existe, toute page mène à `/setup`. Le formulaire demande un **code de configuration** (`A1B2-C3D4-E5F6`, 48 bits aléatoires) tiré à chaque démarrage et affiché **seulement dans la console** du serveur, tant que l'application n'est pas configurée. Il prouve que l'on a la main sur la machine : un autre compte local qui devancerait l'utilisateur sur `/setup` n'a pas le code. Une fois le mot de passe choisi, `/setup` n'est plus accessible.
- **Mot de passe** : au moins 12 caractères, 256 au plus (borne le travail du hachage). Conservé uniquement sous forme d'empreinte **Argon2id** (19 Mo, 2 passes, sel aléatoire), au format PHC qui porte ses paramètres. Jamais journalisé, jamais renvoyé dans une page.
- **Session** : un jeton aléatoire de 244 bits, dans le cookie `worthfold_session` (`HttpOnly`, `SameSite=Lax`, 14 jours, et `Secure` dès que l'adresse publique est en `https` ; pas de `Secure` en `http` local, où le navigateur refuserait de garder le cookie). La base ne garde que l'**empreinte SHA-256** du jeton : une copie de `worthfold.db` ne permet pas de se faire passer pour l'utilisateur. Se déconnecter supprime la session côté serveur ; changer le mot de passe les supprime **toutes**. Les sessions expirées sont retirées à chaque connexion.
- **`Lax` et non `Strict`** : le retour de la banque (`/bank/callback`) est une navigation venue d'un autre site, que `Strict` priverait du cookie. Cela ne rouvre pas le CSRF : une modification est un `POST`, et la garde des requêtes refuse tout `POST` qui ne vient pas de worthfold (`Sec-Fetch-Site`).
- **Essais freinés, par adresse IP** : après 5 échecs de suite (mot de passe, code de configuration, mot de passe actuel) depuis une même adresse, la porte se ferme pour elle une minute, puis le double à chaque échec, jusqu'à un quart d'heure ; un succès remet à zéro cette adresse seulement. Un tiers qui s'acharne ne bloque donc pas le propriétaire. Une adresse qui n'a pas échoué depuis une heure est oubliée, la table est bornée à 10 000 adresses (la plus ancienne part) et une adresse IPv6 compte pour son préfixe /64. En mémoire : tout repart à zéro au redémarrage, ce que le coût du hachage et le code de 48 bits rendent acceptable.
  - **D'où vient l'adresse** : de la connexion TCP. `X-Forwarded-For` est **ignoré**, car le client l'écrit lui-même et changerait d'« adresse » à chaque essai. Derrière un proxy, `WORTHFOLD_PROXY_HOPS=n` déclare `n` proxys de confiance ; chacun ajoute à droite l'adresse de celui qui lui parle, et l'adresse retenue est la `n`-ième en partant de la droite (ce que le client a écrit, à gauche, ne compte pas). Sans cette variable derrière un proxy, tous les clients ont l'adresse du proxy et partagent un seul frein : le comportement d'un compteur global. Sur Kubernetes, l'adresse n'arrive intacte que si le Service de l'Ingress garde l'IP source (`externalTrafficPolicy: Local`) ; sinon c'est celle d'un nœud.
  - **Limite** : un attaquant qui dispose de nombreuses adresses a 5 essais par adresse. Le coût d'Argon2id, la longueur minimale du mot de passe et le code de configuration restent la vraie défense.
- **Message d'échec** : « Mot de passe incorrect. », sans dire s'il y a un autre compte ni ce qui manque.
- **Changer le mot de passe** redemande l'actuel : une session ouverte oubliée ne suffit pas.
- **Mot de passe oublié** : la tâche `WORTHFOLD_RESET_PASSWORD_TASK` (voir [Tâches](tasks.md)) retire le mot de passe et les sessions ; elle suppose l'accès à la machine. Aucune page web ne permet la réinitialisation.
- **Fichiers statiques** (`/static/…`) : accessibles sans session, car la page de connexion en a besoin ; ils ne contiennent aucune donnée.
- **Journaux** : le code de configuration est écrit dans la console au démarrage (seulement avant la première configuration). Ne pas rediriger cette sortie vers un fichier partagé.

## Réseau

- **Écoute locale par défaut** : le serveur se lie à `127.0.0.1`. `WORTHFOLD_BIND_ADDRESS` peut le changer, et l'image le met à `0.0.0.0` (dans un conteneur, c'est ce qui rend le port publié joignable) ; c'est alors le compose qui limite l'exposition, en ne publiant le port que sur `127.0.0.1` de la machine. Ouvrir l'application à d'autres machines demande HTTPS devant et `WORTHFOLD_PUBLIC_URL` (voir [Déploiement](deployment.md#exposer-hors-de-la-machine)).
- **Garde des requêtes** (`guard.rs`, devant toutes les routes) : écouter en local ne suffit pas, car une page d'un autre site ouverte dans le même navigateur peut appeler le serveur local. Le CORS n'y change rien : il empêche de *lire* une réponse, pas d'*envoyer* un formulaire. Deux règles ; toute violation répond 403 et est journalisée :
  - **`Host` connu** : seuls `127.0.0.1`, `localhost` (avec ou sans port) et l'hôte de `WORTHFOLD_PUBLIC_URL`, s'il est défini, sont acceptés, même en lecture. Cela bloque le *DNS rebinding*, où un domaine piégé pointe vers le serveur pour lire les pages comme s'il était worthfold. Derrière un proxy, celui-ci doit transmettre le `Host` d'origine.
  - **HTTPS** : avec une adresse publique en `https`, toutes les réponses portent `Strict-Transport-Security` (un an, sans sous-domaines) et tous les cookies `Secure`. TLS lui-même est terminé par le proxy ou l'Ingress, pas par worthfold.
  - **Modification depuis worthfold** : un `POST` n'est accepté que si `Sec-Fetch-Site` vaut `same-origin` (ou `none`), ou s'il est absent (`curl`). Cela bloque le CSRF. Une autre application sur `localhost` (`same-site`) est refusée aussi. Un lien venant d'un autre site (`GET`) reste accepté : c'est ce qui permet le retour de la banque sur `/bank/callback`.
- **En-têtes** : voir [Web](web.md#en-têtes-et-contenu-de-page). La CSP interdit tout script et tout style qui ne vient pas de l'application.
- **Aucune requête sortante**, sauf vers `api.enablebanking.com` quand la banque est configurée (autorisation, session, opérations, soldes, et la liste des banques d'un pays, qui n'envoie que le code du pays). Pas de CDN, pas de police ni de script distants : htmx et Roboto Flex sont embarqués.

## Contenu de la page (XSS)

- **Échappement** : askama échappe toute valeur insérée dans le HTML, y compris les messages d'erreur renvoyés à htmx (`partials/inline_error.html`) : un message qui cite la saisie ne peut pas injecter de HTML. Aucun HTML n'est construit par concaténation en Rust.
- **Seules exceptions** : `{{ svg|safe }}` dans `partials/chart.html` et `{{ chart_svg|safe }}` dans `forecast_page.html`. Le SVG vient de `chart.rs`, qui n'y insère que des nombres, des dates et des montants mis en forme, et échappe le texte de `chart::message`. Aucun autre `|safe`.
- **Pas de valeur dans du JavaScript** : l'échappement HTML ne protège pas une valeur placée dans un script. Le message d'une confirmation de suppression passe par `hx-confirm`, que htmx affiche tel quel sans l'exécuter. Aucun attribut `hx-on` ni script en ligne : la remise à zéro des formulaires est dans `assets/js/app.js`, qui ne lit aucune valeur du serveur.
- **Message flash** : le cookie `worthfold_flash` (`HttpOnly`, `SameSite=Strict`, une minute) porte un message affiché une fois dans la snackbar. Il n'est pas signé : l'utilisateur peut le forger, mais le texte est échappé à l'affichage, donc au pire il se trompe lui-même.
- **Fichiers téléversés** : un relevé CSV est lu comme du texte et jamais exécuté ni servi à nouveau. Les libellés qu'il contient sont échappés comme toute valeur.

## Base de données

- **SQL** : uniquement des littéraux avec paramètres `?` (sqlx), jamais de valeur concaténée.
- **Migrations** : une migration appliquée ne se modifie jamais (somme de contrôle) ; voir [Développement](development.md).
- **Pas de chiffrement au repos** : PostgreSQL stocke les comptes (y compris le numéro ou l'IBAN saisi, facultatif), les opérations, les libellés et les `session_id` bancaires en clair. L'IBAN est masqué dans les listes, mais entier sur la page du compte. La protection est celle du disque, du volume Docker et des droits d'accès à la base.
- **Accès à la base** : l'adresse de connexion porte le mot de passe. Elle n'est jamais journalisée (les messages d'erreur de connexion ne la reprennent pas). Dans le compose, la base n'est publiée nulle part et son réseau n'a aucune sortie ; seule l'application la joint.
- **Aucun mot de passe par défaut** : le compose refuse de démarrer sans `POSTGRES_PASSWORD`.
- **Sauvegardes** : une sauvegarde contient tout, y compris les `session_id` bancaires et les empreintes de mot de passe. La protéger comme la base (voir [Déploiement](deployment.md#sauvegarde-et-restauration)).
- **Fichiers locaux** : `.env` et `secrets/` sont ignorés par Git et en `0600`. `*.db` reste ignoré (anciennes bases SQLite).

## Secrets et connexion bancaire

- **Un seul vrai secret** : la clé privée RSA d'EnableBanking, dans `secrets/enablebanking.pem` (dossier `0700`, fichier `0600`, ignoré par Git avec `*.pem`). L'identifiant d'application n'est pas un secret. Elle ne se logue jamais, ne passe jamais dans une URL ni dans la base.
- **Configuration** : `.env` (`0600`, ignoré par Git) ne contient que des chemins et des identifiants, jamais la clé elle-même. Les variables sont décrites dans [Développement](development.md#configuration).
- **Jeton d'appel** : un JWT RS256 de 5 minutes, signé à chaque appel, jamais conservé.
- **`session_id`** : stocké en clair dans la base. Il ne sert à rien sans la clé privée, qui seule signe les appels. Pas de coffre-fort : il ne protégerait pas contre un attaquant qui lit déjà le disque, puisque l'application doit pouvoir lire la clé pour fonctionner.
- **Retour de la banque** : le paramètre `state` est un jeton aléatoire à usage unique, enregistré avant la redirection (`bank_pending`) et consommé au retour ; un `state` inconnu ou rejoué est refusé (test HTTP). Le retour est le seul `GET` qui écrit : il est protégé par ce jeton, pas par `Sec-Fetch-Site`.
- **URL de retour** : doit correspondre exactement à celle déclarée chez EnableBanking (`http://localhost:3000/bank/callback` en local, `<WORTHFOLD_PUBLIC_URL>/bank/callback` sinon), sinon la banque refuse.
- **Portée de l'accès** : lecture seule (comptes, soldes, opérations). Aucun ordre de paiement n'existe dans le code. Le consentement dure 90 jours ; le retirer supprime la session côté banque (`DELETE /sessions/{id}`) puis localement.
- **Journaux** : jamais de clé, de jeton, de code d'autorisation, d'IBAN ni de montant. Une erreur technique se journalise en détail (`tracing`) mais s'affiche à l'utilisateur en message générique.

## Dépendances

Les versions sont figées par `Cargo.lock`. Les crates de chiffrement et de requête (`jsonwebtoken`, `reqwest`, `rustls`) sont des bibliothèques réputées ; aucune ne fait de cryptographie maison.

## Écarts connus

Corrections à faire, par ordre d'importance. Cocher à mesure.

- [x] **En-têtes de réponse** : toute réponse porte une CSP (`default-src 'none'`, `script-src 'self'`, `style-src 'self'`, `frame-ancestors 'self'`, sans exception en ligne), `X-Content-Type-Options: nosniff`, `X-Frame-Options: SAMEORIGIN`, `Referrer-Policy: no-referrer` et `Cache-Control: no-store` hors fichiers statiques (`headers.rs`). Pour y arriver, les deux `hx-on` sont remplacés par `assets/js/app.js`, le `<style>` du graphique est dans `app.css`, et htmx est configuré sans évaluation de code ni style injecté. `form-action` reste libre : « Connecter » redirige vers la page de la banque. Vérifié dans Chromium sans violation, et par un test des pages.
- [x] **Droits de la base** : avec SQLite, le fichier était lisible par les autres comptes de la machine (`0644`). Avec PostgreSQL, l'accès passe par un mot de passe et un réseau fermé ; il n'y a plus de fichier à protéger.
- [x] **URL de session dans les journaux** : les erreurs réseau d'EnableBanking sont journalisées sans l'adresse (`without_url`), donc sans le `session_id` ; un test l'impose.
- [ ] **Audit des dépendances.** La commande est documentée dans [Développement](development.md), mais elle n'a pas encore été lancée : `cargo-audit` n'est pas installé sur cette machine. À faire une fois : `cargo install cargo-audit --locked`, puis `cargo audit`.
- [x] **Authentification** : mot de passe Argon2id, sessions à jeton haché, essais freinés, code de configuration (voir ci-dessus). L'exposition derrière un proxy HTTPS est prise en charge (`WORTHFOLD_PUBLIC_URL`, cookie `Secure`, HSTS). Reste peut-être une double authentification, .
- [ ] **Sauvegardes.** Rien ne les automatise : voir [Déploiement](deployment.md#sauvegarde-et-restauration) pour la commande, à planifier. Une sauvegarde en clair copie aussi les `session_id`.

## À chaque nouvelle fonctionnalité

1. Une valeur venue de l'extérieur (formulaire, CSV, banque, URL) passe par un template qui l'échappe, jamais par `format!` dans du HTML ni dans un script.
2. Toute modification est un `POST` ; un `GET` ne modifie rien (sauf le retour de la banque, protégé par `state`).
3. Un secret ne va ni dans le dépôt, ni dans la base, ni dans un journal, ni dans une URL.
4. Un message d'erreur montré à l'utilisateur ne contient aucun détail technique ; le détail va dans les journaux, sans donnée sensible.
5. Un nouvel appel sortant est documenté ici, avec ce qu'il envoie.
