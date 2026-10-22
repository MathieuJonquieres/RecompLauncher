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

### Serveurs et fiche jeu
Providers (`ManifestServerProvider`, `ManualServerProvider`, `LanDiscovery`) → agrégation →
`ServerPinger` (timeout 2 s, sémaphore ≤ 16) → événements UI. Les serveurs sont affichés
dans l'onglet « En ligne » de la fiche jeu, uniquement si le manifeste les déclare. Le
carnet d'amis reste présent dans le core (`FriendsStore`) mais la refonte UI ne l'expose
plus : aucune page Amis, aucun compte, aucun envoi de données.

### Interface (refonte M10)
`crates/app/src/ui` porte le système de design : `theme` (palette/tokens sombre et clair),
`text` (Inter 400/500/600/700), `icons` (Lucide), `components` (boutons, badges, champs,
cartes), `modal`, `art` (visuels abstraits générés, sans jaquette) et `format` (français,
tailles, dates, chemins abrégés). `crates/app/src/views` contient le shell et les écrans
(Catalogue, Bibliothèque, Paramètres, fiche jeu, installation, désinstallation).

La fenêtre est sans décoration système : le shell gère le déplacement, les boutons
réduire/agrandir/fermer et des poignées de redimensionnement invisibles. La cible est
1440×900, le minimum 1180×720. En fenêtre étroite, le catalogue passe à deux colonnes
avec un panneau de détail de 310 px. egui n'expose pas de flou d'arrière-plan : les
modales utilisent un voile opaque renforcé.

Les vues ne font aucune I/O : elles envoient des `Command` réelles (installation,
désinstallation, réparation, mods, serveurs, préférences) et rendent les `Event` reçus.
Aucune donnée de démonstration n'est injectée ; les cartes indisponibles restent
sélectionnables mais leurs actions sont désactivées.

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
