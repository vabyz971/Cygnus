# 0031 — J4 : activation, jeton et confirmations du serveur MCP

- **Date** : 2026-11-03
- **Statut** : proposée (phase A du jalon J4)
- **Exigences concernées** : spec 2.2, 5 (sécurité), 6 (Préférences > Agent, panneau Agent), 8 ; décision 0020 (Q3 : lecture de fichier par l'agent)

## Proposition 3 : activation et jeton

- **Désactivé par défaut**, vérifié par un test : une installation neuve n'ouvre aucun port.
- **Activation** dans Préférences > Agent, par une case « Autoriser un agent à piloter Cygnus Photo ». Activer génère un jeton s'il n'en existe pas.
- **Jeton** : 32 octets aléatoires du système (`getrandom`), affichés **une seule fois** en base64url dans une boîte de dialogue avec un bouton Copier et l'exemple de configuration du client. Seule son **empreinte SHA-256** est enregistrée, dans `~/.config/cygnus/photo/agent.json` (droits 0600 sous Unix). Un jeton perdu ne se relit pas : on le régénère.
- **Révocation** : bouton « Révoquer le jeton » (Préférences et panneau Agent). L'empreinte est effacée, les requêtes suivantes reçoivent 401, et un nouveau jeton n'est créé que sur demande.
- **État visible** : indicateur dans la barre d'état (icône de robot), avec trois états : désactivé (absent), à l'écoute (gris) et en activité (couleur d'accent pendant 2 s après une requête). Une bulle donne l'adresse et le nombre de requêtes.
- **Arrêt immédiat** : un bouton « Arrêter » dans le panneau Agent ferme le port. Les requêtes en cours sont annulées (la connexion se ferme) et les commandes déjà appliquées restent dans l'historique, annulables.
- **Persistance (question Q1)** : je propose que l'activation persiste d'un lancement à l'autre (c'est un choix explicite de l'utilisateur, fait dans les préférences), le serveur démarrant alors avec l'application et l'indicateur le montrant. Alternative plus stricte : le serveur repart désactivé à chaque lancement, et il faut le réactiver depuis le panneau Agent. La consigne interdit de « le démarrer sans action explicite de l'utilisateur » : à trancher.

## Proposition 4 : confirmations et dossiers autorisés

Toute commande qui **lit ou écrit le disque** (`source.import` aujourd'hui ; `document.save` et `document.export` à leur arrivée en J6) demande, **quand elle vient de l'agent**, une confirmation dans l'interface :

- boîte de dialogue non bloquante pour la fenêtre, au premier plan, qui nomme l'action, le chemin complet résolu et l'agent ;
- choix : **Autoriser une fois**, **Autoriser ce dossier** (pour cette session de l'application, ou toujours si la case est cochée), **Refuser** ;
- les dossiers autorisés « toujours » sont listés dans Préférences > Agent, où l'on peut les retirer.

**Contrôle des chemins**, avant la confirmation et avant toute lecture :

- chemin canonique (liens symboliques résolus) ; un chemin autorisé l'est par son dossier canonique, et un lien qui sort du dossier est refusé ;
- fichiers ordinaires seulement (pas de périphérique ni de tube), taille bornée par les limites du décodeur (0029) ;
- un chemin relatif est résolu depuis le dossier du document, jamais depuis le dossier de travail du processus.

**Utilisateur absent et délais :**

- la requête de l'agent reste en attente pendant **60 s** au plus (réglable de 10 à 300 s). Sans réponse, elle est refusée avec l'erreur « confirmation non donnée dans le délai », et l'agent reçoit une erreur d'outil explicite, jamais un blocage ;
- une seule confirmation à la fois : une autre requête disque de l'agent pendant l'attente est refusée tout de suite (« une confirmation est déjà en attente ») ;
- les commandes sans accès disque ne demandent pas de confirmation : leurs effets sont visibles, marqués « agent » dans l'historique et annulables.

**Mode sans fenêtre** : aucune confirmation possible, donc seuls les dossiers `--allow-dir` sont lisibles ou écrivibles ; tout le reste est refusé.

**Alternative écartée : passer par l'élicitation MCP** (demander la confirmation au client de l'agent). Ce serait l'agent qui demande la permission à son propre utilisateur, dans son interface. La confirmation doit venir de l'utilisateur de Cygnus Photo, dans Cygnus Photo.

## Risques

- **Persistance de l'activation** (Q1) : un serveur qui redémarre seul est plus pratique mais moins visible.
- **Lassitude des confirmations** : l'option « Autoriser ce dossier » limite les demandes ; elle élargit d'autant les droits de l'agent.
- **Jeton dans la configuration du client** : il y est stocké en clair par le client d'agent, hors de notre contrôle. La révocation est la parade.
