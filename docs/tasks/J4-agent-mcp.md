# Consigne : jalon J4, Agent MCP

À placer dans `docs/tasks/J4-agent-mcp.md`. À lancer **après** la fin de B6 (réglages et objets incorporés, scénario photo sans fenêtre), car J4 s'appuie sur le catalogue de commandes et le scénario de J3.

Phrase à donner à l'agent : « Lis `AGENTS.md` et `docs/spec.md`, puis réalise la **phase A** de la consigne `docs/tasks/J4-agent-mcp.md`. Ne commence pas la phase B avant ma validation écrite. »

## Contexte

Le catalogue de commandes typées et sérialisables (J2, J3) existe, avec son schéma généré. J4 expose ces commandes à un agent IA par un serveur MCP intégré à l'application : c'est une des deux fonctionnalités phares du produit (section 2.2 de la spec). L'agent n'a **aucun pouvoir que l'utilisateur n'a pas**.

Sections de `docs/spec.md` concernées : 2.2, 4.8 (HIS-1, HIS-3), 5 (sécurité), 6 (Préférences > Agent, panneau Agent), 7.2, 8, 11 (décision ouverte sur le transport), 10 (jalon J4). Reprendre aussi les décisions sur le résolveur de commandes (0020), le catalogue et l'historique (0022).

**Ce jalon est une zone à supervision étroite** (sécurité du serveur MCP, `AGENTS.md` section 6) : deux phases, et la phase B ne commence qu'après validation écrite de la phase A.

## Phase A : proposition (aucun code de production)

Rédiger dans `docs/decisions/` des propositions numérotées, avec alternatives écartées et risques, sur :

1. **Protocole et bibliothèque** : version actuelle de la spécification MCP à viser et bibliothèque Rust éventuelle. **Vérifier l'état actuel sur les sources officielles** plutôt que de se fier à la mémoire ; comparer écrire le serveur soi-même et utiliser une bibliothèque (maturité, licence compatible GPL v3, taille).
2. **Transport** : HTTP local avec jeton, socket local ou stdio (décision ouverte n° 7 de la spec). Justifier. Pour un serveur HTTP local, traiter explicitement : liaison à l'interface de boucle locale uniquement, **validation des en-têtes `Host` et `Origin`** contre les attaques par navigateur (rebinding DNS), comparaison du jeton à temps constant.
3. **Activation et jeton** : désactivé par défaut ; activation dans *Préférences > Agent* ; jeton généré à l'activation, affiché une fois, révocable ; état visible dans la barre d'état ; bouton d'arrêt immédiat dans le panneau Agent.
4. **Confirmations** : toute action qui lit ou écrit le disque (`source.import`, `document.save`, `document.export`) demande une confirmation à l'utilisateur par défaut, avec la possibilité d'autoriser un dossier donné. Définir le comportement quand l'utilisateur est absent, et les délais d'attente.
5. **Outils** : génération automatique depuis le catalogue ; quelles commandes sont exclues par défaut (les commandes de développement comme `pixels.fill` et `pixels.write`) ; noms, descriptions, schémas ; limites de taille des paramètres.
6. **Ressources** : arbre du document en JSON, aperçu rendu en PNG à une taille demandée (avec une taille maximale), histogramme, liste de l'historique, schéma des commandes. Dire comment l'aperçu est produit sans bloquer l'interface.
7. **Contenu non fiable** : les noms de calques, métadonnées et textes du document sont des données, jamais des instructions. Décrire comment ils sont présentés à l'agent (par exemple clairement étiquetés comme données) et ce qui est fait pour limiter l'injection d'instructions.
8. **Concurrence** : l'utilisateur et l'agent modifient le document en même temps. Tout passe par le bus, donc par un ordre unique ; décrire les conflits possibles, l'effet sur l'historique (HIS-3 : entrées marquées « agent »), l'annulation d'une action de l'agent par l'utilisateur, et les opérations longues (rendu, import).
9. **Interface** : panneau Agent (état du serveur, journal des requêtes et de leur résultat, bouton d'arrêt, révocation du jeton), réglages dans Préférences, indicateur dans la barre d'état. Fiche de design dans `docs/views/`.
10. **Mode sans fenêtre** : le même serveur doit pouvoir tourner sans fenêtre (`cygnus mcp`) pour les agents de développement, avec les mêmes règles de sécurité.
11. **Tests** : client de test en Rust pour la CI (conformité au protocole, chaque outil, chaque ressource), tests de sécurité (sans jeton, mauvais jeton, mauvais `Host` ou `Origin`, serveur désactivé, jeton révoqué), scénario photo exécuté par le client de test.
12. **Documentation** : `docs/mcp.md` pour l'utilisateur (activer le serveur, configurer un client, limites).
13. **Découpage de la phase B** en petites étapes vérifiables.

Terminer par un résumé de dix lignes au plus, avec les **trois décisions les plus risquées** et les questions à trancher, puis **s'arrêter**.

## Phase B : implémentation (après validation écrite de la phase A)

Selon la proposition validée :

- `crates/mcp-server` : serveur générique alimenté par un catalogue de commandes, transport, authentification, journalisation.
- Génération des outils et des ressources depuis le catalogue de `photo`.
- Confirmations et autorisations de dossiers.
- `apps/photo` : Préférences > Agent, panneau Agent, indicateur d'état, marquage « agent » dans l'historique.
- `apps/cli` : mode `cygnus mcp` sans fenêtre.
- Tests unitaires, tests de sécurité, test de bout en bout avec le client de test.
- `docs/mcp.md`.

## Interdit dans ce jalon

- Exposer le serveur au-delà de la boucle locale, ou le démarrer sans action explicite de l'utilisateur.
- Un outil qui contourne le bus de commandes ou qui donne à l'agent un pouvoir absent de l'interface.
- Toute télémétrie.
- Écrire à la main des outils qui dupliquent le catalogue.
- Toute décision de la phase A prise sans validation.

## Critère de sortie

- Phase A validée par écrit.
- Le serveur est **désactivé par défaut**, vérifié par un test.
- Un client MCP exécute le **scénario photo complet** (de J3) contre l'application, vérifié par un test de bout en bout en CI, puis une fois à la main par l'utilisateur avec un vrai agent.
- Tous les tests de sécurité de la phase A passent.
- Chaque action de l'agent apparaît dans l'historique, marquée comme telle, et l'utilisateur peut l'annuler.
- `cargo fmt`, `clippy`, `test`, `build`, en-têtes et `cargo deny` passent ; la CI est verte sur Linux, Windows et macOS.

## En fin de tâche

Compte rendu court : ce qui est fait, les écarts avec la proposition validée, les limites de sécurité connues et les risques pour la suite.
