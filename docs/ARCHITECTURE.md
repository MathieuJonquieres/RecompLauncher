# Architecture — RecompLauncher

## Vue d'ensemble

```
┌──────────────────────────── app (eframe) ────────────────────────────┐
│ views (egui)  ── Command ──▶  BackendBridge  ──▶  AppContext (core) │
│      ▲                              │                                │
│      └────────── Event ◀────────────┘  (tokio::sync::mpsc)           │
└──────────────────────────────────────────────────────────────────────┘
```

- `crates/core` ne dépend d'aucune crate UI. Il contient la logique métier, les providers,
  l'accès SQLite, le réseau et les processus.
- `crates/app` est le binaire eframe. Il ne fait aucune I/O : il envoie des `Command` et
  affiche les `Event` reçus. Un `egui::Context::request_repaint()` est déclenché à chaque
  événement.
- `crates/testkit` fournit des helpers de test (fixtures, contexte temporaire).

## Threads

| Composant | Thread |
|---|---|
| UI egui | thread principal |
| Runtime tokio | pool multi-thread créé dans `main` |
| SQLite | opérations bloquantes via `spawn_blocking`, mutex non tenu sur `.await` |
| Téléchargements | tâches tokio, progression par `watch`/`mpsc` |
| Processus de jeu | `tokio::process::Command`, suivi via une tâche |

## Flux principaux

### Catalogue
`CatalogService::refresh()` → `join_all(providers.fetch_all())` → validation/déduplication
→ sauvegarde SQLite (`catalog_cache`, `catalog_meta`) → `Event::CatalogUpdated`.
Au démarrage : `load_cached()` (même expiré) → affichage immédiat hors ligne, puis refresh
en arrière-plan.

### Installation
Résolution release GitHub → sélection asset OS/arch → téléchargement streaming avec reprise
et `CancellationToken` → vérification SHA-256 si publié → extraction sécurisée dans un
dossier temporaire → `rename` atomique → enregistrement `library_games` → sélection et
vérification du fichier utilisateur → `chmod +x` sur Unix. En cas d'échec : suppression du
dossier temporaire, version précédente conservée.

### Lancement
`GameLauncher` construit `Vec<String>` d'arguments (jamais de shell), résout les mods selon
`ModsSpec::strategy`, puis lance `Command::new(exe).args(args).current_dir(dir)` avec un
environnement explicite.

### Amis / serveurs
Providers (`ManifestServerProvider`, `ManualServerProvider`, `LanDiscovery`) → agrégation →
`ServerPinger` (timeout 2 s, sémaphore ≤ 16) → événements UI. Le carnet d'amis reste local.

## Sécurité

- Tout chemin externe (archive, mod, manifeste) passe par `SafeRelPath`.
- Extraction : refus de `..`, chemins absolus, préfixes de lecteur, liens symboliques,
  noms réservés Windows ; limites de taille, nombre d'entrées et ratio de compression.
- HTTP : HTTPS uniquement, rustls, redirections re-vérifiées contre l'allowlist, taille de
  body plafonnée.
- Processus : arguments en vecteur, `cwd` contrôlé, pas de shell.
- Secrets : trousseau système, jamais loggés.

## Structure des fichiers

Voir `AGENT_SPEC_recomp_launcher_rust.md` §5. Les modules `core` sont découpés par domaine
(`catalog`, `library`, `net`, `db`, `online`, `social`, `adapters`, `archive`).
