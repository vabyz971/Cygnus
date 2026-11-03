# 0033 — J4 : concurrence, interface, tests, documentation et découpage

- **Date** : 2026-11-03
- **Statut** : proposée (phase A du jalon J4)
- **Exigences concernées** : spec 2.2, 4.8 (HIS-1, HIS-3), 6, 10 (J4) ; décisions 0020, 0022, 0025, 0030 à 0032

## Proposition 8 : concurrence

- **Un seul ordre** : le document et le bus appartiennent au fil de l'interface. Le fil du serveur ne touche jamais au document : il envoie chaque requête dans une file (canal) au fil de l'interface, qui l'applique au début de l'image suivante par `resolve_and_execute`, puis renvoie le résultat par un canal de réponse. L'interface se réveille quand une requête arrive (`request_repaint`).
- **Geste de l'utilisateur en cours** (glissement d'un curseur) : les commandes de l'agent attendent la fin du geste. Sinon le bus les rangerait dans le geste ouvert, et une annulation de l'utilisateur emporterait aussi l'action de l'agent. L'attente est bornée à 10 s ; au-delà, la requête est refusée (« l'utilisateur est en train de modifier le document »).
- **Conflits** : l'agent désigne des nœuds par identifiant. Si l'utilisateur a supprimé ou verrouillé le nœud entre-temps, la commande est refusée avec l'erreur du cœur (nœud inconnu, verrouillé), sans effet. Le refus est atomique (0010).
- **Historique** (HIS-3) : chaque entrée de l'agent porte la source `Agent`, montrée par une icône et un texte dans le panneau Historique (fait en B8). Les entrées de l'agent ne fusionnent jamais avec celles de l'utilisateur (gestes distincts). L'utilisateur annule une action de l'agent comme les siennes.
- **Opérations longues** :
  - **import** : le décodage se fait **dans le fil du serveur**, avant d'entrer dans la file, par le même résolveur (`Import`). Seule la commande déjà résolue passe au fil de l'interface ;
  - **fusion vers le bas** : elle se fait sur le fil de l'interface (moteur de rendu de la fenêtre) ; le temps est borné par la taille du document (mesuré en phase B) ;
  - **aperçu** : asynchrone (0032).
- **Mode sans fenêtre** : un seul fil, les requêtes stdio sont traitées dans l'ordre d'arrivée.

## Proposition 9 : interface

Fiche de design : `docs/views/panneau-agent.md`. En résumé :

- **Panneau Agent** (onglet ancrable) : état du serveur (désactivé, à l'écoute, en activité) avec l'adresse, gros bouton **Arrêter** ou **Démarrer**, bouton **Révoquer le jeton**, et journal des requêtes (heure, outil ou ressource, résumé des paramètres, résultat : appliqué, refusé avec la raison, ou en attente de confirmation). Le journal est en mémoire, borné à 500 lignes et effaçable.
- **Préférences > Agent** : activation, port, jeton (générer, révoquer), dossiers autorisés, délai de confirmation, inclusion des outils de développement (désactivée).
- **Barre d'état** : indicateur (0031).
- **Confirmation** : boîte de dialogue (0031).

## Proposition 11 : tests

- **Client de test en Rust** (`crates/mcp-server/tests/client.rs`), qui parle HTTP et stdio sans bibliothèque cliente tierce, pour tester le serveur tel qu'un client le voit.
- **Conformité** : `server/discover` ; en-têtes obligatoires ; `tools/list` égal au catalogue, sans les commandes de développement ; chaque outil appelé au moins une fois avec des paramètres valides et invalides ; chaque ressource lue ; `resultType`, `ttlMs` et `cacheScope` présents.
- **Sécurité**, chaque cas étant un test :
  - sans jeton → 401 ; mauvais jeton → 401 ; jeton révoqué → 401 ;
  - `Host` étranger → 421 ; `Origin` étranger → 403 ; `Origin` absent avec un bon jeton → accepté ;
  - serveur désactivé → aucun port ouvert (connexion refusée) ; liaison effective uniquement sur la boucle locale ;
  - corps trop gros → 413 ; plus de 20 requêtes par seconde → 429 ;
  - outil de développement absent par défaut ;
  - accès disque de l'agent sans confirmation → refusé à l'expiration du délai ; lien symbolique qui sort d'un dossier autorisé → refusé.
- **Bout en bout** : le scénario photo de J3 (0027, 0029) exécuté par le client de test :
  - contre `cygnus mcp` sur stdio, le dossier des images étant passé par `--allow-dir` ;
  - contre une session avec serveur HTTP dans le même processus (sans fenêtre, file traitée par le test).
  
  Le rendu final est comparé à la golden image du scénario. L'historique montre chaque entrée marquée « agent », et un test annule une action de l'agent du côté de l'utilisateur.
- CI : Linux, Windows et macOS, comme le reste.

## Proposition 12 : documentation

`docs/mcp.md`, en français, pour l'utilisateur :

- activer le serveur et copier le jeton ;
- configurer un client (HTTP avec en-tête `Authorization` ; ligne de commande `cygnus mcp` pour stdio) ;
- confirmations et dossiers autorisés ;
- ce que l'agent peut et ne peut pas faire ;
- limites (contenu non fiable, aperçu réduit) ;
- arrêt et révocation.

## Proposition 13 : découpage de la phase B

| Étape | Contenu | Vérification |
| --- | --- | --- |
| **C1** | Bus : champ `public` des entrées (0032) ; panneau Historique et ressource sur cette forme | Annuler puis rétablir garde le chemin d'import ; tests de 0022 verts |
| **C2** | `crates/mcp-server` générique sur `rmcp` : outils depuis le catalogue, fournisseur abstrait, stdio ; `cygnus mcp` | `tools/list` égal au catalogue ; scénario sur stdio |
| **C3** | Transport HTTP : liaison locale, hôte, origine, jeton à temps constant, limites | Tous les tests de sécurité HTTP |
| **C4** | Confirmations et dossiers autorisés, contrôle des chemins | Délai, refus, liens symboliques |
| **C5** | Ressources : arbre, aperçu réduit asynchrone, histogramme, historique, schéma ; étiquetage du contenu non fiable | Chaque ressource lue par le client de test |
| **C6** | Application : file vers le fil de l'interface, Préférences > Agent, panneau Agent, indicateur, boîte de confirmation | Captures ; serveur désactivé par défaut (test) |
| **C7** | Bout en bout HTTP, `docs/mcp.md`, essai à la main avec un vrai agent par l'utilisateur | CI verte sur trois systèmes ; compte rendu |

## Risques

- La file vers le fil de l'interface ajoute au plus une image de latence par commande de l'agent. C'est acceptable pour un agent, mais à mesurer.
- Les tests HTTP ouvrent de vrais ports en CI : il faut un port choisi par le système (`:0`) dans les tests, pour éviter les collisions.
