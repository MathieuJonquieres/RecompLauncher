# RecompLauncher

Launcher de bureau générique pour jeux **recompilés** et **décompilés** (Rust + egui/eframe).

> **État du projet** : jalons M0 à M9 terminés (catalogue hors ligne, installation,
> lancement, mods, fiche Wikipédia, serveurs/Amis, jeux externes, i18n FR/EN).
> Voir `PROGRESS.md` pour les preuves et `docs/DECISIONS.md` pour les arbitrages.

---

## Français

### Mission

Parcourir un catalogue de projets recomp/decomp, gérer une bibliothèque de jeux
(installés, manuels ou externes), lancer les jeux avec leurs mods et consulter des
serveurs multijoueurs — sans jamais distribuer de contenu de jeu.

### Ce que le launcher ne fait **jamais**

- Télécharger, héberger ou lier une **ROM, ISO, image disque, clé de chiffrement,
  firmware ou asset copyrighté**.
- Copier ou envoyer vos fichiers de jeu. Vous sélectionnez votre propre dump ; le
  launcher vérifie uniquement son hash (SHA-1 / SHA-256 / CRC32) et enregistre son
  chemin local.
- Contourner un DRM, scraper du contenu protégé par login ou ignorer `robots.txt`.
- Fournir un émulateur, une clé ou un firmware pour les jeux sans recomp publique.

### Limites connues

**Super Smash Bros. Ultimate** et **Mario Kart 8 Deluxe** n'ont, à notre
connaissance (octobre 2026), **aucune recompilation/décompilation publique
complète**. Ils n'apparaissent donc pas dans le catalogue par défaut. Le mécanisme
générique **« Jeu externe »** permet de déclarer vous-même un exécutable que vous
possédez déjà, sans que le launcher ne fournisse ou ne configure d'émulateur, de
clé ou de firmware. Cette fonctionnalité est disponible dans la page Bibliothèque
(bouton « Ajouter un jeu externe »).

### Fonctionnalités

- Catalogue complet virtualisé, filtres, recherche, tri, cache SQLite et mode hors ligne.
- Installation sécurisée depuis les releases officielles (hash, reprise, extraction
  contrôlée, rollback), réparation et désinstallation conservant `userdata/` et `mods/`.
- Lancement des jeux avec prise en compte des mods (dossier canonique, activation,
  réordonnancement, miroir `native_dir` ou arguments).
- Fiche jeu enrichie par l'API Wikipédia (attribution **CC BY-SA 4.0**, lien source,
  cache 30 jours, repli sur le manifeste si le réseau échoue).
- Options en ligne **uniquement si le manifeste les déclare** : liste publique JSON,
  découverte mDNS LAN, ajout manuel `host:port`, ping TCP (timeout 2 s, 16 max en
  parallèle) et connexion via `join_template`.
- Page Amis avec carnet **entièrement local** (aucun compte, aucun envoi de données).
- Jeux externes et adaptateurs par slug (`GameAdapter`/`AdapterRegistry`).
- Interface FR/EN (Fluent) pour la navigation, le catalogue, la bibliothèque, les amis
  et les paramètres ; les fiches détaillées gardent encore des libellés français.
- Thème clair/sombre, proxy, cache, export des logs.

### Téléchargements

Les artefacts de release sont produits par GitHub Actions pour Windows x86_64,
Linux x86_64 et macOS x86_64 à chaque tag `v*` (`.github/workflows/release.yml`).
Ils contiennent uniquement le binaire du launcher et ce README : aucun jeu, ROM,
clé, firmware ni asset copyrighté.

### Sources de données

Le brief ne fournissait pas d'URL vérifiable pour « le site recomp ». Le catalogue
par défaut est donc un manifeste embarqué et vérifié manuellement
(`resources/catalog.seed.json`), complété optionnellement par un manifeste local ou
distant. Les binaires proviennent exclusivement des **releases officielles GitHub**
des projets. Détails et licences : `docs/DATA_SOURCES.md`.

### Développement

Prérequis : Rust stable (MSRV 1.80), un linker C (MSVC ou MinGW), CMake/Ninja pour
`rusqlite` (feature `bundled`).

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p app
```

Sous Windows sans Visual Studio Build Tools, utiliser la cible GNU :

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
$env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-gnu"
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\chemin\vers\mingw64\bin;$env:PATH"
cargo build --workspace
```

Les données sont stockées dans `%APPDATA%\RecompLauncher` (Windows),
`~/.local/share/RecompLauncher` (Linux) ou `~/Library/Application Support/RecompLauncher`
(macOS). Un fichier `portable.flag` à côté de l'exécutable active le mode portable.

---

## English

Generic desktop launcher for **recompiled** and **decompiled** games (Rust +
egui/eframe).

### Mission

Browse a catalog of recomp/decomp projects, manage a game library (launcher-managed,
manual or external), launch games with mods and browse multiplayer servers — while
never distributing game content.

### What the launcher never does

- Download, host or link a **ROM, ISO, disc image, decryption key, firmware or
  copyrighted asset**.
- Copy or upload your game files. You select your own dump; the launcher only
  verifies its hash (SHA-1 / SHA-256 / CRC32) and stores its local path.
- Bypass DRM, scrape login-protected content, or ignore `robots.txt`.
- Provide an emulator, key or firmware for games without a public recomp.

### Known limitations

**Super Smash Bros. Ultimate** and **Mario Kart 8 Deluxe** have, to our knowledge
(October 2026), **no complete public recompilation/decompilation**. They are
therefore absent from the default catalog. The generic **“External game”**
mechanism lets you register an executable you already own; the launcher does not
provide or configure any emulator, key or firmware. It is available from the
Library page (“Add external game”).

### Features

- Full virtualised catalog, filters, search, sort, SQLite cache and offline mode.
- Secure installation from official releases (hashes, resume, guarded extraction,
  rollback), repair and uninstall preserving `userdata/` and `mods/`.
- Game launching with detected mods (canonical folder, enable/reorder, `native_dir`
  mirror or argument strategy).
- Wikipedia-enriched game details (CC BY-SA 4.0 attribution, source link, 30-day
  cache, manifest-only fallback when the network fails).
- Online options **only when the manifest declares them**: JSON public list, mDNS
  LAN discovery, manual `host:port`, TCP ping (2 s timeout, max 16 concurrent) and
  joining through `join_template`.
- Friends page with a **fully local** book (no account, no upload).
- External games and per-slug adapters (`GameAdapter`/`AdapterRegistry`).
- FR/EN UI (Fluent) for navigation, catalog, library, friends and settings; detailed
  game pages still contain some French labels.
- Light/dark theme, proxy, cache and log export.

### Release downloads

GitHub Actions builds Windows x86_64, Linux x86_64 and macOS x86_64 artifacts for
every `v*` tag (`.github/workflows/release.yml`). They contain only the launcher
binary and this README: no game, ROM, key, firmware or copyrighted asset.

### Data sources

The brief did not include a verifiable URL for the “recomp” site. The default
catalog is therefore a hand-verified embedded manifest
(`resources/catalog.seed.json`), optionally extended by a local or remote manifest.
Binaries come exclusively from projects' **official GitHub releases**. See
`docs/DATA_SOURCES.md` for details and licenses.

### Development

Requirements: stable Rust (MSRV 1.80), a C linker (MSVC or MinGW), CMake/Ninja for
`rusqlite` (`bundled` feature).

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p app
```

Application data lives in `%APPDATA%\RecompLauncher` (Windows),
`~/.local/share/RecompLauncher` (Linux) or `~/Library/Application Support/RecompLauncher`
(macOS). A `portable.flag` file next to the executable enables portable mode.

---

## Licence

Le code du launcher est distribué sous MIT OR Apache-2.0. Les projets listés dans
le catalogue conservent leurs licences respectives (voir
`resources/catalog.seed.json` et `docs/DATA_SOURCES.md`).
