# AGENT_SPEC — RecompLauncher (launcher de jeux recompilés/décompilés, Rust)

> Ce fichier est ta source de vérité. Lis-le en entier avant d'écrire du code.
> En cas de conflit entre ce fichier et ton intuition, ce fichier gagne. En cas d'ambiguïté non couverte, choisis l'option la plus simple, note-la dans `docs/DECISIONS.md`, et continue (ne bloque pas).

---

## 0. Mission

Construire un **launcher de bureau en Rust** qui permet de :

1. **Parcourir un catalogue complet** des jeux recompilés / décompilés listés sur le site « recomp » (voir §3 : la source exacte doit être vérifiée).
2. **Gérer une bibliothèque** de jeux ajoutés (installés via le launcher ou installés manuellement ailleurs), avec fiche détaillée, gestion de mods et options en ligne.
3. **Trouver des serveurs multijoueurs** et des amis (page « Amis »).

Le launcher doit être **générique** : il ne code pas en dur un jeu précis. Chaque jeu = une entrée de manifeste + éventuellement un petit adaptateur.

---

## 0 bis. Suivi d'avancement et reprise (`PROGRESS.md`) — OBLIGATOIRE

Tu peux être **interrompu à tout moment** (crédits épuisés, limite de contexte, coupure). Un autre agent — ou toi-même plus tard, sans aucun souvenir — doit pouvoir reprendre le travail **uniquement à partir du dépôt**. Le fichier `PROGRESS.md` à la racine du dépôt est ta mémoire externe. Il est aussi important que le code.

### Protocole de début de session (toujours, dans cet ordre)
1. Lis ce fichier (`AGENT_SPEC`) en entier.
2. Lis `PROGRESS.md`, puis `docs/DECISIONS.md`.
3. Lis `git log --oneline -20` et `git status` (travail non commité ?).
4. Exécute la **commande de vérification** indiquée dans `PROGRESS.md` (`cargo build`, `cargo test`, `cargo clippy`…) pour connaître l'état réel.
5. **Réconcilie** : si `PROGRESS.md` contredit le dépôt (jalon marqué fini mais tests rouges, par ex.), **le dépôt a raison** : corrige `PROGRESS.md` d'abord, puis continue.
6. Reprends à la section « ▶ Prochaine action », pas ailleurs.

Si `PROGRESS.md` n'existe pas : tu démarres à zéro. Crée-le **immédiatement** (modèle ci-dessous) avant d'écrire la moindre ligne de code.

### Quand mettre à jour `PROGRESS.md`
Ne l'écris pas « à la fin » : la fin peut ne jamais venir. Mets-le à jour :
- **avant** de commencer une tâche non triviale (« ▶ En cours : … ») ;
- **après chaque sous-tâche terminée** (module qui compile, test qui passe) ;
- **après chaque décision** ou blocage rencontré ;
- **au moins toutes les ~30 minutes** de travail ou tous les ~5 fichiers modifiés, même si rien n'est fini ;
- dès que tu sens que le budget/contexte devient serré : arrête de coder, fais un **checkpoint complet** (état propre, WIP commité, « Prochaine action » précise).

Chaque mise à jour est **commitée avec le code concerné** (`chore(progress): …` ou dans le même commit). Pour un travail inachevé : commit `wip: …` plutôt que laisser des changements non commités. Le dépôt doit **toujours compiler** à un checkpoint ; si ce n'est pas possible, dis-le explicitement dans « État de la build ».

### Modèle de `PROGRESS.md` (à créer tel quel puis tenir à jour)

```markdown
# PROGRESS — RecompLauncher

_Dernière mise à jour : <date ISO> — session n°<N>_

## ▶ Prochaine action (la plus importante du fichier)
<Une action concrète et sans ambiguïté, ex. : « Dans crates/core/src/library/install.rs,
implémenter `verify_hash()` (§6.5) ; le test `install::tests::hash_mismatch` existe
mais est #[ignore] ; le retirer et le faire passer. »>

## État de la build
- `cargo build` : OK / KO (détail)
- `cargo test` : <x> passés / <y> échoués / <z> ignorés
- `cargo clippy -- -D warnings` : OK / KO
- Dernier commit sain : <hash court>
- Travail non commité : oui/non (si oui, quoi)

## Jalons (§8 du spec)
| # | Jalon | Statut | Preuve (test / commit / démo) |
|---|---|---|---|
| M0 | Squelette + CI | ✅ / 🟡 en cours / ⬜ à faire / ⛔ bloqué | `abc1234`, `cargo test -p core` |
| M1 | … | … | … |
(… jusqu'à M9)

## Checklist « Terminé » (§12)
- [ ] … (recopier les cases du §12 et les cocher au fur et à mesure)

## En cours (détail)
- Tâche : …
- Fichiers touchés : …
- Ce qui reste à faire pour la finir : …

## Décisions prises (résumé ; détail dans docs/DECISIONS.md)
- <date> — <décision> — <pourquoi>

## Blocages / questions ouvertes
- <ex. URL du site « recomp » non fournie → utilisation de ManifestProvider (§3)>

## Pièges connus / choses à ne pas refaire
- <ex. « sevenz-rust échoue sur archives X : utiliser … »>

## Commandes utiles
- Build : `cargo build --workspace`
- Tests : `cargo nextest run --workspace`
- Lint : `cargo clippy --workspace --all-targets -- -D warnings`
- Lancer l'app : `cargo run -p app`

## Journal de sessions (le plus récent en haut)
- Session N (<date>) : fait … ; laissé … ; prochaine étape …
```

### Règles de véracité du suivi
- Un jalon n'est ✅ **que si** sa « preuve » est reproductible (test vert, commande, commit). Sinon 🟡.
- Ne jamais effacer l'historique : ajoute au journal ; tu peux condenser les anciennes sessions.
- « ▶ Prochaine action » doit être **réécrite à chaque checkpoint** et rester assez précise pour qu'un agent sans contexte agisse sans poser de question.
- `PROGRESS.md` ne contient ni secrets, ni clés API, ni chemins personnels.
- Si tu es coupé en plein milieu : l'état laissé dans `PROGRESS.md` + `git` doit suffire. C'est ton critère de qualité.

---

## 1. Règles non négociables (légal / périmètre)

Ces règles priment sur toute demande ultérieure.

1. **Le launcher ne télécharge, n'héberge, ne lie et ne contient JAMAIS** : ROM, ISO, images disque, clés de chiffrement (prod.keys, etc.), firmwares, assets copyrightés.
2. Les projets recomp/decomp distribuent du **code moteur uniquement**. L'utilisateur fournit **ses propres fichiers de jeu** (dump de sa copie légale). Le launcher doit :
   - demander à l'utilisateur de **sélectionner son fichier** (ROM/ISO/dossier) ;
   - **vérifier son hash** (SHA-1 / SHA-256 / CRC32 selon ce que le projet exige) ;
   - ne rien faire d'autre avec : pas d'upload, pas de copie hors du dossier du jeu, pas de téléchargement alternatif.
3. Les binaires sont téléchargés **depuis les releases officielles du projet** (GitHub Releases ou équivalent), jamais depuis des miroirs tiers.
4. **Smash Bros Ultimate et Mario Kart 8 Deluxe** : à ma connaissance (recherche d'octobre 2026), il n'existe **pas de recompilation/décompilation publique complète** de ces titres. Ne les invente pas. Règles :
   - Ils n'apparaissent dans le catalogue **que si la source de données les liste réellement**.
   - Pour supporter « ces genres de jeux », implémente le mécanisme générique **« Jeu externe »** (§6.6) : l'utilisateur pointe lui-même vers un exécutable/launcher qu'il possède déjà. Le launcher ne fournit, n'indique et ne configure **aucun émulateur, clé ou firmware** pour ces titres.
   - Documente cette limite dans le README.
5. Ne contourne aucun DRM, ne scrape rien qui soit protégé par login, respecte `robots.txt` et les rate limits.

Si une tâche te demande de violer ce qui précède : refuse cette partie, note-la dans `docs/DECISIONS.md`, continue le reste.

---

## 2. Stack technique (imposée — ne pas changer sans raison écrite)

| Domaine | Choix |
|---|---|
| Langage | **Rust stable**, édition **2021** (ou 2024 si la toolchain le permet), MSRV **1.80** déclarée dans `rust-version` |
| Build | **Cargo workspace** + `rust-toolchain.toml` (canal `stable`, composants `clippy`, `rustfmt`) |
| UI | **egui / eframe** (+ `egui_extras` pour tableaux/images), rendu `wgpu` ou `glow` |
| Runtime async | **tokio** (multi-thread) ; l'UI ne bloque jamais, elle communique avec le backend par canaux (`tokio::sync::mpsc` / `watch`) |
| Réseau HTTP | **reqwest** (features `rustls-tls`, `json`, `stream`, `gzip`, `http2`) — **pas d'OpenSSL** |
| Sérialisation | **serde** + **serde_json** |
| Validation de schéma | **jsonschema** (schéma JSON Draft 2020-12) |
| Base locale | **rusqlite** (feature `bundled`) + **rusqlite_migration** ; accès via un thread/`spawn_blocking` dédié |
| Archives | **zip**, **tar** + **flate2**/**xz2**, **sevenz-rust** (extraction sécurisée maison par-dessus, §10) |
| Hash | **sha1**, **sha2**, **crc32fast**, **md-5** (RustCrypto) |
| Processus | `tokio::process::Command` (arguments en `Vec`, jamais de shell) |
| Erreurs | **thiserror** (libs/crates `core`) + **anyhow** (binaire/frontière) |
| Logs | **tracing** + **tracing-subscriber** + **tracing-appender** (fichier tournant) |
| Chemins | **directories** (`ProjectDirs`) ; mode portable (§5) |
| Parsing HTML | **scraper** (uniquement dans le provider concerné, isolé) |
| Découverte LAN | **mdns-sd** + **socket2**/`tokio::net::UdpSocket` pour le broadcast |
| i18n | **fluent-templates** (Fluent `.ftl`) ; **FR par défaut + EN** |
| Manette (phase finale) | **gilrs** |
| Tests | `cargo test` + **cargo-nextest**, **wiremock** (serveur HTTP mocké), **insta** (snapshots), **proptest** (parseurs/chemins), **tempfile** |
| Qualité | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, **cargo-deny** (licences + advisories), **cargo-audit** |
| CI | GitHub Actions : Windows (MSVC), Linux (x86_64), macOS |
| Packaging | **cargo-packager** ou **cargo-dist** : zip portable + installeur Windows, AppImage Linux, .dmg macOS (phase finale) |

Règles de langage :
- `#![forbid(unsafe_code)]` dans tous les crates **sauf** si un `unsafe` est justifié par écrit dans `docs/DECISIONS.md`.
- Pas de `unwrap()` / `expect()` / `panic!` hors tests et hors invariants documentés ; `?` partout.
- Pas de `block_on` dans du code async, pas de `std::thread::sleep` dans une tâche tokio.
- Dépendances ajoutées : justifier en une ligne dans `docs/DECISIONS.md`, vérifier licence (cargo-deny).

Interdits : Electron/Tauri/webview comme UI principale, dépendance Python/Node au runtime, OpenSSL, `async-std`, macros procédurales maison sans nécessité.

Cibles : **Windows 10+ et Linux x86_64 d'abord** ; macOS et ARM en phase finale (beaucoup de projets recomp n'ont pas de build ARM/macOS → le catalogue doit filtrer par plateforme).

---

## 3. Source de données « recomp » — À VÉRIFIER EN PREMIER

⚠️ Le brief dit « le site recomp » sans URL. **Je n'ai pas pu identifier de façon certaine une API officielle**. Il existe des listes éditoriales (guides web, listes communautaires GitHub/Reddit/ResetEra) mais pas, à ma connaissance, d'API JSON stable. Ne suppose donc rien.

### Étape 0 (obligatoire, avant tout code UI)
1. Demande/relis l'URL exacte du site « recomp » donnée par l'utilisateur. Si absente, note le blocage dans `docs/DECISIONS.md`, utilise l'option B ci-dessous et continue.
2. Cherche, dans l'ordre : (a) une API ou un flux JSON/RSS/sitemap officiel ; (b) un dépôt GitHub avec la liste en données structurées ; (c) à défaut, des pages HTML publiques.
3. Documente le résultat dans `docs/DATA_SOURCES.md` (URL, format, licence/CGU, fréquence de mise à jour).

### Architecture imposée : trait `CatalogProvider` (pattern adaptateur)

```rust
// crates/core/src/catalog/provider.rs
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ProviderInfo { pub id: String, pub name: String, pub url: String }

#[async_trait]
pub trait CatalogProvider: Send + Sync {
    fn info(&self) -> ProviderInfo;
    /// Catalogue COMPLET (pagination gérée en interne). Les entrées invalides sont
    /// ignorées et loggées, jamais fatales.
    async fn fetch_all(&self) -> Result<Vec<GameEntry>, CatalogError>;
}
```

Providers à implémenter :
- **A. `RecompSiteProvider`** : parse la source réelle identifiée à l'étape 0 (API si elle existe, sinon HTML avec parseur isolé et tests `insta` sur fixtures sauvegardées ; casser le parseur ne doit jamais faire paniquer l'app).
- **B. `ManifestProvider`** *(fallback garanti)* : lit un **manifeste JSON local ou distant** (`catalog.json`, schéma §4). Livre un `catalog.seed.json` embarqué (`include_str!` / `rust-embed`) avec des projets réels et vérifiés (ex. N64Recomp / Zelda64Recomp, projets de décompilation avec port PC, etc. — **chaque entrée doit pointer vers un dépôt réel, vérifié à la main ; aucune URL inventée**).
- **C. `GitHubReleasesProvider`** (utilitaire) : à partir d'un `owner/repo`, récupère les dernières releases et assets (API GitHub, token optionnel, cache ETag via `If-None-Match`).

Un `CatalogService` agrège les providers (`futures::future::join_all`), déduplique (clé = `slug`), met en cache SQLite (TTL 6 h, rafraîchissement manuel), et fonctionne **hors ligne** avec le dernier cache.

« Catalogue COMPLET » = **toutes** les entrées du provider, avec virtualisation UI (`ScrollArea::show_rows`/équivalent, pas de limite arbitraire), + filtres : plateforme d'origine (N64, GBA, Xbox 360, PC, PS1…), type (recomp / decomp / port natif), état (jouable / bêta / WIP), OS compatible, recherche texte, tri.

---

## 4. Modèle de données

### 4.1 Manifeste de jeu (`GameEntry`, JSON)

```json
{
  "schema": 1,
  "slug": "zelda64-recomp",
  "title": "The Legend of Zelda: Majora's Mask",
  "original_platform": "N64",
  "kind": "recomp",
  "status": "playable",
  "project": { "name": "Zelda64Recomp", "repo": "https://github.com/<owner>/<repo>", "license": "..." },
  "releases": {
    "source": "github",
    "assets": [
      { "os": "windows", "arch": "x86_64", "pattern": "*Windows*.zip" },
      { "os": "linux",   "arch": "x86_64", "pattern": "*Linux*.zip" }
    ]
  },
  "user_files": {
    "required": true,
    "description": "Dump de votre propre copie du jeu",
    "accepted_hashes": [ { "algo": "sha1", "value": "<hash>", "label": "USA" } ],
    "prompt_ext": [".z64", ".n64", ".v64"]
  },
  "launch": { "exe": { "windows": "Zelda64Recompiled.exe", "linux": "Zelda64Recompiled" }, "args": [] },
  "mods": { "supported": true, "dir_name": "mods", "strategy": "native_dir", "format_hint": ".nrm", "docs_url": "..." },
  "online": { "supported": false, "modes": [] },
  "metadata": { "wikipedia_title": "The Legend of Zelda: Majora's Mask", "igdb_id": null }
}
```

Structs Rust correspondantes :

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEntry {
    pub schema: u32,
    pub slug: String,
    pub title: String,
    pub original_platform: String,
    pub kind: GameKind,          // enum: Recomp, Decomp, NativePort
    pub status: GameStatus,      // enum: Playable, Beta, Wip
    pub project: Project,
    pub releases: Releases,
    pub user_files: Option<UserFiles>,
    pub launch: LaunchSpec,
    pub mods: ModsSpec,
    pub online: OnlineSpec,
    pub metadata: MetadataRefs,
    // champs inconnus ignorés => pas de #[serde(deny_unknown_fields)] sur le manifeste
}
```

Règles : `schema` versionné ; **champs inconnus ignorés** (forward-compatible) ; validation avec un JSON Schema dans `/resources/schema/game.schema.json` ; une entrée invalide est **ignorée et loggée** (`tracing::warn!`), jamais fatale. Utilise des **enums** et des **newtypes** (`Slug`, `Sha1Hex`, `SafeRelPath`) plutôt que des `String` nues.

### 4.2 Base SQLite (tables minimales)

`catalog_cache`, `library_games` (id, slug, install_path, version, installed_at, source: `managed|manual|external`, last_played, playtime_s), `mods` (game_id, name, path, enabled, order), `servers` (id, game_slug, name, address, port, source, last_ping_ms, players, max_players, favorite), `friends` (id, display_name, notes), `friend_handles` (friend_id, service, handle), `metadata_cache` (slug, json, fetched_at), `settings` (k/v).

Migrations idempotentes (`rusqlite_migration`, fichiers SQL embarqués), testées sur base vide **et** base de la version précédente.

---

## 5. Architecture du code

```
recomp-launcher/
├─ Cargo.toml                (workspace), rust-toolchain.toml, deny.toml, clippy.toml
├─ AGENT_SPEC.md             (ce fichier), PROGRESS.md (§0 bis, tenu à jour en continu)
├─ docs/                     DECISIONS.md, DATA_SOURCES.md, ARCHITECTURE.md
├─ resources/                schema/, sql/, catalog.seed.json, i18n/*.ftl, icons/
├─ crates/
│  ├─ core/                  ← logique métier, AUCUNE dépendance UI
│  │   └─ src/
│  │       ├─ lib.rs, error.rs, paths.rs, settings.rs, ctx.rs   (AppContext)
│  │       ├─ net/           http_client.rs (retry, ETag, timeouts), download.rs
│  │       ├─ catalog/       provider.rs, recomp_site.rs, manifest.rs,
│  │       │                 github_releases.rs, service.rs, filter.rs
│  │       ├─ library/       library.rs, install.rs, launcher.rs, mods.rs, external.rs
│  │       ├─ metadata/      provider.rs, wikipedia.rs, (igdb.rs optionnel)
│  │       ├─ online/        provider.rs, lan.rs, manifest_servers.rs,
│  │       │                 manual.rs, pinger.rs, local_server.rs
│  │       ├─ social/        friends.rs
│  │       ├─ adapters/      trait GameAdapter + implémentations par slug
│  │       ├─ archive/       extraction sécurisée
│  │       └─ db/            connexion, migrations, repos
│  ├─ app/                   ← binaire (eframe) : fenêtres, vues, view-models
│  │   └─ src/ main.rs, app.rs, backend_bridge.rs, views/{catalog,library,game,friends,settings}.rs
│  └─ testkit/               fixtures, serveurs mockés, helpers de test
└─ .github/workflows/
```

Règles d'architecture :
- **Séparation stricte** : `core` ne dépend d'aucune crate UI. L'UI n'appelle que des **commandes** envoyées au backend et reçoit des **événements** (`enum Command` / `enum Event` sur canaux). Aucune I/O réseau/disque dans la boucle de rendu.
- L'UI demande un repaint via `egui::Context::request_repaint()` quand un événement arrive.
- Erreurs : `thiserror` dans `core` (un enum par module, `#[from]` pour la propagation), `anyhow` uniquement dans `app`.
- **Pas de globals mutables** (`static mut`, `lazy_static` mutable) : injection via `AppContext` (`Arc<…>`).
- Traits `async` **object-safe** via `async_trait` quand utilisés en `dyn`, sinon `async fn` natif dans les traits.
- Tous les chemins via `paths.rs` (`directories::ProjectDirs`) ; mode **portable** si un fichier `portable.flag` est à côté de l'exécutable.
- Tout chemin issu d'une donnée externe (manifeste, archive, mod) passe par le newtype **`SafeRelPath`** (§10).

Dossiers utilisateur :
```
<AppData>/RecompLauncher/
  launcher.db, cache/, logs/
  games/<slug>/<version>/      ← binaires installés par le launcher
  games/<slug>/mods/           ← dossier mods (§6.3)
  games/<slug>/userdata/       ← sauvegardes/configs si le jeu le permet
```

---

## 6. Fonctionnalités (par page)

Navigation : barre latérale avec 4 entrées — **Catalogue**, **Bibliothèque**, **Amis**, **Paramètres**.

### 6.1 Page « Jeux » (Catalogue)
- Liste/grille virtualisée de **tous** les jeux du `CatalogService`.
- Carte : titre, plateforme d'origine, badge de type (recomp/decomp), état, OS compatibles, miniature (si disponible légalement ; sinon placeholder généré).
- Filtres + recherche + tri (§3). Bouton « Actualiser » + indicateur « dernière MAJ ».
- Détail d'un jeu : description, lien dépôt, versions disponibles, prérequis (fichiers utilisateur), bouton **Ajouter à la bibliothèque** / **Installer**.
- Les jeux non compatibles avec l'OS courant sont **visibles mais grisés** avec la raison.
- Hors ligne : afficher le cache avec bandeau d'avertissement.

### 6.2 Page « Bibliothèque »
- Liste des jeux ajoutés avec **badge de provenance** : `Installé par le launcher` / `Installation manuelle` / `Jeu externe`.
- **Ajout manuel** : l'utilisateur pointe un dossier/exécutable existant (sélecteur natif via **rfd**) → le launcher l'associe à un `slug` du catalogue (ou crée une entrée personnalisée).
- Actions : Jouer, Mettre à jour (comparer version installée / dernière release avec **semver** quand possible), Réparer (revérifier fichiers), Désinstaller (confirmation ; ne supprime **jamais** `userdata/` sans accord explicite), Ouvrir le dossier (**opener**).
- **Fiche de jeu** au clic (§6.3).

### 6.3 Fiche d'un jeu (menu détaillé)
Trois onglets :

**a) Infos** — données récupérées via le trait `MetadataProvider` :
- `WikipediaProvider` : API REST Wikipedia (`/api/rest_v1/page/summary/{titre}`) — résumé, image, année, développeur/éditeur si disponibles. Respecter la licence **CC BY-SA** : afficher l'attribution + lien vers la page source. Définir un `User-Agent` explicite.
- Provider optionnel (IGDB/autre) activé uniquement si l'utilisateur fournit une clé API dans les paramètres.
- Cache 30 jours en base ; échec réseau → afficher le manifeste seul.
- N'utilise **pas** de jaquettes/captures officielles non libres sauf si fournies par l'utilisateur.

**b) Mods**
- Bouton **« Créer le dossier mods »** : `tokio::fs::create_dir_all` sur `games/<slug>/mods/` (ou le chemin défini dans `mods.dir_name` du manifeste) si absent, puis bouton « Ouvrir ».
- Les mods sont **pris en compte** : à chaque lancement, le launcher scanne le dossier, liste les mods détectés (extensions du `format_hint`), permet activer/désactiver/réordonner (table `mods`), puis applique selon `mods.strategy` :
  - `"native_dir"` : le jeu lit lui-même `mods/` → on s'assure que le dossier existe et que seuls les mods activés y sont visibles (dossier `mods.disabled/` pour les autres) ;
  - `"args"` : passer les chemins en arguments (template défini dans le manifeste) ;
  - `"none"` : l'UI affiche « Mods non supportés par ce projet » et désactive les actions.
- Import par glisser-déposer (egui `dropped_files`) d'un fichier mod → copié dans le dossier. **Aucune exécution** du contenu du mod par le launcher ; avertir que les mods tiers sont à risque.
- Anti-traversée de chemin sur toute copie/extraction (§10).

**c) En ligne**
- Section « Options en ligne » selon `online.modes` du manifeste : `local_server`, `public_server`, `p2p`, `none`.
- Menu pour **installer/configurer** : serveur local (télécharger le binaire serveur officiel du projet si une release existe, le lancer en sous-processus avec port configurable, afficher logs/arrêt) ; serveur public (choisir dans la liste du §6.4) ; sinon message clair « pas d'option en ligne connue pour ce jeu ».
- N'affiche une option en ligne que si le manifeste la déclare. **N'invente pas** de support multijoueur.

### 6.4 Page « Amis » (multijoueur)
Il n'existe pas de service central d'amis commun à ces projets. On construit donc une page honnête, basée sur des **sources de serveurs** :

1. **Serveurs** : liste agrégée par jeu depuis le trait `ServerProvider` :
   - `ManifestServerProvider` : liste JSON (locale/distante) de serveurs communautaires ;
   - `LanDiscovery` : découverte LAN (UDP broadcast / mDNS selon ce que les jeux exposent ; protocole configurable par jeu) ;
   - `ManualServerProvider` : ajout manuel `host:port`.
2. `ServerPinger` : mesure de latence + (si le protocole du jeu le permet via adaptateur) joueurs/capacité. Timeout 2 s, concurrence limitée (≤ 16, `tokio::sync::Semaphore`).
3. **Amis** : carnet **local** (pseudo + handles par service). Si un serveur expose sa liste de joueurs, mettre en évidence les serveurs où un ami est présent (matching par pseudo, avec avertissement que c'est approximatif). Aucune donnée d'ami n'est envoyée à un serveur tiers.
4. Actions : « Rejoindre » (lance le jeu avec l'adresse selon le template du manifeste), favoris, filtres par jeu / latence / non vide.
5. Aucune authentification, aucun compte, aucun backend à nous.

### 6.5 Installation (`InstallManager`)
Pipeline : résoudre release → choisir l'asset (OS/arch via `std::env::consts`) → télécharger en **streaming** (`reqwest` `bytes_stream`, reprise via `Range`, progression, annulation par `CancellationToken` de `tokio-util`) → **vérifier intégrité** (hash publié si dispo) → extraire dans un dossier temporaire (`tempfile`) → déplacer atomiquement (`rename`) → enregistrer en base → demander le fichier utilisateur (§1.2) et vérifier son hash → positionner le bit exécutable sur Linux/macOS (`PermissionsExt`).
Rollback complet en cas d'échec. Conserver la version précédente jusqu'à succès de la nouvelle.

### 6.6 Jeu externe
Entrée de bibliothèque de type `external` : l'utilisateur choisit un exécutable + arguments + dossier de travail. Aucune logique propre au jeu, aucune fourniture de composants tiers. Sert aux titres sans recomp publique (ex. cas §1.4) que l'utilisateur sait déjà lancer lui-même. Étiqueté clairement « non géré par le launcher ».

### 6.7 Paramètres
Langue, thème clair/sombre, dossier d'installation, proxy, clé GitHub/IGDB optionnelles, mode portable, vider le cache, export des logs.

---

## 7. Compatibilité multi-jeux (exigence centrale)

Test d'acceptation : le même code doit gérer, **uniquement via manifestes**, au moins :
1. un jeu **N64 recomp** (ex. type Zelda64Recomp) avec fichier utilisateur + mods ;
2. un projet **decomp avec port PC** (GBA/N64/autre) avec fichier utilisateur sans mods ;
3. un **jeu PC natif** sans fichier utilisateur ;
4. un **jeu externe** (§6.6).

Si un jeu demande une logique spéciale, crée un **adaptateur** implémentant le trait `GameAdapter` (hooks : `pre_launch`, `build_args`, `post_install`, `parse_server_status`) enregistré par `slug` dans un `AdapterRegistry` (`HashMap<Slug, Arc<dyn GameAdapter>>`). Pas de `if slug == "..."` éparpillés dans le code.

---

## 8. Plan d'exécution (ordre strict, un jalon = un commit/PR testé)

| # | Jalon | Critère de fin |
|---|---|---|
| M0 | Workspace Cargo + CI + fenêtre eframe vide + `tracing` | `cargo build`, `fmt`, `clippy -D warnings` verts sur Win + Linux |
| M1 | Étape 0 sources de données + `docs/DATA_SOURCES.md` + schéma JSON + validation | Tests schéma verts |
| M2 | `HttpClient`, `ManifestProvider`, `CatalogService` + cache SQLite | Catalogue chargé hors ligne |
| M3 | UI Catalogue (virtualisée, filtres, détail) | Démo ≥ 500 entrées fluide (jeu de test généré) |
| M4 | `InstallManager` + `Library` + page Bibliothèque + vérif hash | Install/désinstall/réparer OK sur manifeste de test |
| M5 | `GameLauncher` + `ModManager` + bouton « Créer le dossier mods » | Mods détectés & pris en compte au lancement |
| M6 | Fiche jeu + `WikipediaProvider` + attribution | Infos affichées, cache OK |
| M7 | Online : serveur local, serveurs publics, LAN, manuel | Page Amis fonctionnelle avec mock |
| M8 | Jeu externe + adaptateurs + test des 4 cas §7 | Tests d'acceptation verts |
| M9 | i18n FR/EN, packaging, README, finitions | Artefacts de release CI |

N'attaque pas le jalon N+1 tant que N n'est pas vert. Ne fais pas de refactor global hors jalon.

À chaque jalon : (1) avant de commencer, passe-le en 🟡 dans `PROGRESS.md` ; (2) découpe-le en sous-tâches listées dans « En cours » ; (3) à la fin, passe-le en ✅ avec sa preuve, mets à jour « Prochaine action » vers le jalon suivant, et commite (§0 bis).

---

## 9. Qualité

- Couverture visée : ≥ 70 % sur `core` (mesurée avec **cargo-llvm-cov**).
- Tests obligatoires : parseurs (fixtures + `insta`), validation de schéma, migrations SQLite, extraction d'archives (y compris archives malveillantes), vérif de hash, résolution d'assets par OS, scan de mods, `SafeRelPath` (avec `proptest`).
- Aucun test ne dépend du réseau réel : **wiremock** pour HTTP.
- `cargo clippy --all-targets -- -D warnings` propre ; `cargo deny check` et `cargo audit` verts en CI ; **Miri** sur les modules de manipulation de chemins/bytes si du `unsafe` est jamais introduit.
- UI : 60 fps en scroll du catalogue ; démarrage < 2 s hors cache froid ; aucun frame bloqué > 16 ms par de l'I/O.
- Accessibilité : navigation clavier + **manette** (`gilrs`, phase finale), contraste suffisant, textes traduisibles.

---

## 10. Sécurité

- Extraction d'archives : passer chaque entrée par `SafeRelPath::try_new()` qui **refuse** `..`, chemins absolus, préfixes de lecteur Windows, liens symboliques sortants, noms réservés Windows (`CON`, `NUL`…) ; **limiter** la taille décompressée totale, le nombre d'entrées et le ratio de compression (zip bomb). Ne jamais utiliser `extract_all` brut d'une lib sans ces garde-fous.
- Téléchargements : HTTPS uniquement, vérification TLS **non désactivable** (`danger_accept_invalid_certs` interdit, vérifié par un test/lint grep en CI), hash vérifié si fourni, domaines autorisés par provider (allowlist : `github.com`, `api.github.com`, `objects.githubusercontent.com`, domaine du site recomp, `wikipedia.org`), redirections limitées et re-vérifiées contre l'allowlist.
- Lancement : `tokio::process::Command::new(exe).args(&args)` — liste d'arguments, jamais de `sh -c`, `cwd` = dossier du jeu, variables d'environnement explicites.
- Manifestes distants : validés par schéma + taille max (limiter la lecture du body) ; contenu traité comme **non fiable** (jamais exécuté, jamais utilisé comme chemin sans `SafeRelPath`).
- Secrets (clés API) : stockés via le trousseau système (**keyring**), jamais dans les logs (implémenter `Debug` masqué ou utiliser **secrecy**).
- Aucune télémétrie. Aucun envoi de données utilisateur.

---

## 11. Conventions de code

- Style `rustfmt` par défaut ; modules en `snake_case`, types en `PascalCase`, constantes en `SCREAMING_SNAKE_CASE`.
- Préférer les types forts (newtypes, enums) aux `String`/`bool` ; `#[non_exhaustive]` sur les enums publics du manifeste.
- Pas de `clone()` gratuit : passer des références ou des `Arc`; pas de `Mutex` tenu à travers un `.await` (utiliser `tokio::sync` ou restructurer).
- Chaque module public a un commentaire `//!` d'en-tête (rôle, thread/tâche, ownership) et chaque item public non trivial un `///`.
- Commits : Conventional Commits (`feat:`, `fix:`, `test:`, `docs:`, `chore:`).
- Pas de logique métier dans `app/views/*` : seulement affichage + envoi de `Command`.

---

## 12. Définition de « terminé »

- [ ] Les 3 pages (Jeux, Bibliothèque, Amis) + Paramètres fonctionnent.
- [ ] Catalogue complet issu des providers, filtrable, utilisable hors ligne.
- [ ] Fiche jeu avec données Wikipédia + attribution.
- [ ] Bouton « Créer le dossier mods » + prise en compte des mods au lancement.
- [ ] Options en ligne (local / public) uniquement quand déclarées.
- [ ] Page Amis : serveurs (LAN, liste, manuel) + carnet d'amis local.
- [ ] Les 4 cas de §7 passent en test d'acceptation.
- [ ] Aucune ROM/clé/firmware/asset copyrighté dans le dépôt ni téléchargé.
- [ ] README (FR+EN) avec limites connues, dont le statut de Smash Ultimate / Mario Kart 8 Deluxe (§1.4).
- [ ] CI verte (fmt, clippy, tests, deny, audit), artefacts Windows + Linux.
- [ ] `PROGRESS.md` à jour : tous les jalons ✅ avec preuve, « Prochaine action » = « Projet terminé, rien à reprendre ».

---

## 13. En cas de doute — checklist anti-dérive

1. Est-ce que ça touche à du contenu copyrighté ? → **stop** (§1).
2. Est-ce que j'ai vérifié que cette URL/ce projet existe vraiment ? → **sinon ne l'ajoute pas**.
3. Est-ce dans le jalon courant ? → sinon, note-le dans `docs/DECISIONS.md` (« idée future »).
4. Est-ce que ça bloque le thread UI ? → passe par une `Command` asynchrone.
5. Est-ce testable sans réseau ? → sinon, ajoute un mock (wiremock).
6. Est-ce que ça fonctionne via manifeste et pas via du code spécifique à un jeu ? → sinon, adaptateur.
7. Est-ce que j'ai introduit un `unwrap()`, un `unsafe` ou un chemin non validé ? → corrige ou justifie par écrit.
8. Est-ce que `PROGRESS.md` reflète ce que je viens de faire, et est-ce que « ▶ Prochaine action » permettrait à un agent sans contexte de reprendre ? → sinon, mets-le à jour **maintenant**, avant de continuer.

Ne rapporte **jamais** une fonctionnalité comme terminée sans test ou démonstration reproductible.
