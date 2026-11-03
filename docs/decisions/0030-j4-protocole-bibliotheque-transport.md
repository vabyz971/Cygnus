# 0030 — J4 : protocole, bibliothèque et transport du serveur MCP

- **Date** : 2026-11-03
- **Statut** : proposée (phase A du jalon J4)
- **Exigences concernées** : spec 2.2, 5 (sécurité), 7.2, 8, 11 (décision ouverte n° 7), 10 (J4)

## Sources vérifiées (2026-11-03)

- Spécification MCP : la révision stable est **2026-07-28** (journal des changements : <https://modelcontextprotocol.io/specification/2026-07-28/changelog>), qui succède à 2025-11-25. Changements qui nous touchent : la poignée de main `initialize` disparaît (la version du protocole et les capacités du client voyagent dans le `_meta` de chaque requête), `server/discover` devient obligatoire, les sessions et l'en-tête `Mcp-Session-Id` disparaissent, chaque résultat porte un `resultType`, les résultats de ressources exigent `ttlMs` et `cacheScope`, et l'ancien transport HTTP+SSE est déprécié.
- Transport Streamable HTTP (<https://modelcontextprotocol.io/specification/2026-07-28/basic/transports>) : le serveur **doit** valider l'en-tête `Origin` de toute connexion (403 sinon) contre le rebinding DNS, **devrait** écouter uniquement sur la boucle locale et **devrait** authentifier toute connexion. Les en-têtes `MCP-Protocol-Version`, `Mcp-Method` et `Mcp-Name` (pour `tools/call` et `resources/read`) sont exigés. Chaque requête est un POST indépendant ; GET et DELETE répondent 405.
- Transport stdio : messages JSON séparés par des fins de ligne, rien d'autre sur la sortie standard, journal libre sur la sortie d'erreur, arrêt quand l'entrée se ferme.
- Bibliothèque Rust officielle : **`rmcp` 3.8.0** (dépôt <https://github.com/modelcontextprotocol/rust-sdk>, publiée le 2026-10-26), licence **Apache-2.0** (compatible GPL v3), Rust 1.88 minimum, édition 2024. Elle met en œuvre 2026-07-28 et reste compatible avec 2025-11-25. Transports par fonctionnalités Cargo : `transport-io` (stdio) et `transport-streamable-http-server` (un service Tower à monter dans un routeur HTTP). Elle dépend de `tokio`.

## Proposition 1 : protocole et bibliothèque

- **Version visée** : 2026-07-28, avec la compatibilité 2025-11-25 qu'offre la bibliothèque (beaucoup de clients d'agents n'auront pas encore migré).
- **Bibliothèque** : `rmcp` pour la couche protocole (types, JSON-RPC, découverte, stdio et service Streamable HTTP), dans `crates/mcp-server` uniquement. Le service HTTP est monté derrière **notre propre filtre** (hôte, origine, jeton, taille) écrit et testé dans ce dépôt, de sorte que la sécurité ne dépende pas des réglages par défaut de la bibliothèque.
- `crates/mcp-server` reste générique : il ne dépend que de `crates/commands` (graphe de 7.4). Les outils viennent du catalogue (`Catalog`), l'exécution d'un fournisseur abstrait (trait) mis en œuvre par l'application et par la ligne de commande.
- Dépendances nouvelles à vérifier avec `cargo deny` avant de les figer : `rmcp` (Apache-2.0), `tokio` (MIT), un serveur HTTP minimal (`hyper` ou `axum`, MIT), `getrandom` (MIT ou Apache-2.0) pour le jeton.

**Alternative écartée : écrire le protocole nous-mêmes.** La révision 2026-07-28, sans état, est plus simple qu'avant (un POST par message, pas de session), et un serveur JSON-RPC maison tiendrait en quelques centaines de lignes sans `tokio`. Mais le protocole a beaucoup changé entre 2025-06, 2025-11 et 2026-07 : suivre seuls ses évolutions et la compatibilité avec les clients existants coûterait plus que la dépendance. On garde l'option de remplacer `rmcp` si sa maturité déçoit, la frontière étant le trait du fournisseur.

## Proposition 2 : transport (décision ouverte n° 7)

**Deux transports, selon l'usage :**

| Usage | Transport | Pourquoi |
| --- | --- | --- |
| Application avec fenêtre (agent qui pilote la session ouverte de l'utilisateur) | **Streamable HTTP local**, avec jeton | Le client se connecte à un processus déjà lancé ; c'est le seul transport standard le permettant |
| Mode sans fenêtre `cygnus mcp` (agents de développement) | **stdio** | Le client lance le processus lui-même : aucune surface réseau, la sécurité est celle du processus parent |

**Règles du serveur HTTP de l'application :**

- liaison **uniquement** à `127.0.0.1` et `[::1]`, jamais à `0.0.0.0` (vérifié par un test sur l'adresse effective) ;
- port configurable dans Préférences > Agent, par défaut fixe (pour que la configuration du client reste valable) ; refus clair si le port est pris ;
- **en-tête `Host`** exigé et égal à `127.0.0.1:<port>`, `localhost:<port>` ou `[::1]:<port>`, sinon 421 (protection contre le rebinding DNS, en plus de l'origine) ;
- **en-tête `Origin`** : absent (cas des clients d'agents) ou présent dans une liste blanche, **vide par défaut** ; toute autre origine reçoit 403. Un navigateur envoie toujours `Origin` sur une requête intersite, donc une page web ne peut pas atteindre le serveur ;
- **jeton** dans `Authorization: Bearer <jeton>`, comparé **à temps constant** (comparaison de l'empreinte SHA-256 du jeton reçu avec l'empreinte stockée, octet par octet sans sortie anticipée) ; 401 sinon ;
- corps limité à 1 Mio (413 au-delà), en-têtes vérifiés (`MCP-Protocol-Version`, `Mcp-Method`, `Mcp-Name`) par la bibliothèque, GET et DELETE en 405 ;
- au plus 20 requêtes par seconde (429 au-delà), contre un agent qui boucle.

**Alternative écartée : socket local (socket Unix, tube nommé).** Plus sûr en théorie (droits du système de fichiers), mais ce n'est pas un transport MCP standard : aucun client d'agent ne sait s'y connecter sans pont. Un pont stdio vers socket reste possible plus tard sans changer le serveur.

**Alternative écartée : stdio pour l'application.** Le client devrait lancer l'application lui-même : il ne peut pas piloter la session que l'utilisateur a déjà ouverte.

## Proposition 10 : mode sans fenêtre

`cygnus mcp [--document <script.json>] [--allow-dir <dossier>]…` lance le même serveur générique sur stdio, sur une session sans fenêtre (document, bus, moteur de rendu ouvert au premier besoin comme `cygnus render`). Mêmes outils, mêmes ressources, mêmes refus. Sans interface pour confirmer, **tout accès disque hors des dossiers `--allow-dir` est refusé** (0031). La sortie standard ne porte que des messages MCP ; le journal va sur la sortie d'erreur.

## Risques

- **Surface d'attaque du serveur HTTP** dans une application de bureau : autres processus locaux (le jeton les arrête), pages web (origine et hôte les arrêtent). Le serveur reste désactivé par défaut.
- **Évolution rapide de `rmcp`** (version 3.x, changements d'API entre versions majeures) : on fige la version exacte et on isole la bibliothèque dans `crates/mcp-server`.
- **`tokio` dans une application egui** synchrone : le serveur tourne dans son propre fil avec son exécuteur ; aucune tâche asynchrone ne touche l'interface (0033, concurrence).
