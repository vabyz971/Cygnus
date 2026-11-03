# Fiche de vue : panneau Agent, Préférences > Agent, indicateur d'état

Proposition de la phase A de J4 (décisions 0030 à 0033), à valider avant toute mise en œuvre.

## Panneau Agent (onglet ancrable, menu Fenêtre)

```text
┌ Agent ──────────────────────────────────────────┐
│ ● À l'écoute · http://127.0.0.1:47820/mcp       │
│ [ Arrêter ]                [ Révoquer le jeton ] │
│ ─────────────────────────────────────────────── │
│ 14:32:05  layer.create      « Courbes »   ✓      │
│ 14:32:06  source.import     photo.png     ⏳ confirmation │
│ 14:32:41  source.import     photo.png     ✗ délai dépassé │
│ 14:33:02  document/preview  1024 px       ✓      │
│                                    [ Effacer ]  │
└─────────────────────────────────────────────────┘
```

- **Pastille d'état** : grise (désactivé : « Serveur désactivé, voir Préférences > Agent »), verte (à l'écoute), couleur d'accent (requête en cours).
- **Bouton principal** : Arrêter (ferme le port tout de suite) ou Démarrer (si activé dans les préférences).
- **Journal** : une ligne par requête, la plus récente en bas, 500 lignes au plus. Un clic sur une ligne d'action appliquée sélectionne l'entrée correspondante dans le panneau Historique.
- Les noms de calques et les chemins sont affichés comme du texte ordinaire, tronqués à 40 caractères.

## Préférences > Agent

- Case « Autoriser un agent à piloter Cygnus Photo » (décochée par défaut).
- Port (nombre, 1024 à 65535).
- Jeton : « Générer un nouveau jeton » (dialogue d'affichage unique, bouton Copier, exemple de configuration du client), « Révoquer ».
- Dossiers autorisés : liste, boutons Ajouter et Retirer.
- Délai de confirmation : 10 à 300 s (60 par défaut).
- Case « Exposer les outils de développement » (décochée, avec un avertissement).

## Boîte de confirmation

« L'agent demande à lire le fichier `/home/…/photo.png`. » avec les boutons **Refuser** (par défaut, touche Échap), **Autoriser une fois** et **Autoriser ce dossier**, et la case « toujours ». Un compte à rebours montre le délai restant. La boîte ne bloque pas la fenêtre : on peut regarder le document avant de répondre.

## Barre d'état

Icône de robot à droite, à côté de « GPU » : absente quand le serveur est désactivé, grise à l'écoute, couleur d'accent pendant 2 s après une requête. La bulle donne l'adresse et le nombre de requêtes de la session.
