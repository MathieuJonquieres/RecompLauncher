# PROGRESS — RecompLauncher

_Dernière mise à jour : 2026-10-21 — session n°1_

## ▶ Prochaine action (la plus importante du fichier)

Jalon **M4**. Dans `crates/core/src/library/` :
1. créer `Library` (CRUD `library_games`) et ses tests SQLite ;
2. créer `hash.rs` (`verify_file` SHA-1/SHA-256/CRC32/MD5 + tests) ;
3. créer `archive/` (extraction sécurisée zip/tar.gz/tar.xz/7z, `SafeRelPath`, limites
   taille/entrées/ratio, tests d'archives malveillantes) ;
4. créer `net/download.rs` (streaming, reprise `Range`, progression,
   `CancellationToken`, vérification SHA-256) ;
5. créer `library/install.rs` (pipeline résolution → download → extraction temp →
   rename atomique → DB → fichier utilisateur, rollback) et son test wiremock avec
   une archive ZIP générée ;
6. brancher `Command::Install` / `Command::RefreshLibrary` / `Command::SelectUserFile`
   et la page Bibliothèque, puis passer M4 ✅ avec `cargo test --workspace` vert.

## État de la build

- `cargo build --workspace` : **OK** (Windows GNU local ; CI MSVC/Linux/macOS)
- `cargo test --workspace` : **43 passés / 0 échoué / 0 ignoré** (core) + 0 tests app/testkit
- `cargo clippy --workspace --all-targets -- -D warnings` : **OK**
- `cargo fmt --all -- --check` : **OK**
- Dernier commit sain : `040a7b7` (squelette README) — le présent travail va être commité
- Travail non commité : oui, M0–M3 (workspace, core, UI, docs, CI)
- Vérification manuelle : `target/debug/recomp-launcher.exe` démarre et reste ouvert ;
  base SQLite + logs créés dans `%APPDATA%\RecompLauncher`

## Jalons (§8 du spec)

| # | Jalon | Statut | Preuve (test / commit / démo) |
|---|---|---|---|
| M0 | Workspace + CI + fenêtre eframe vide + tracing | ✅ | `cargo build --workspace`, `cargo clippy`, lancement manuel de l'exe |
| M1 | Sources de données + schéma JSON + validation | ✅ | `catalog::manifest` : 8 tests, seed de 6 projets vérifiés, snapshot insta |
| M2 | HttpClient, ManifestProvider, CatalogService + cache | ✅ | 43 tests core (wiremock, ETag, retries, cache hors ligne, migration v1→v2) |
| M3 | UI Catalogue (virtualisée, filtres, détail) | ✅ | `show_rows` + test 500 entrées, lancement app, détail/panneau, bandeau hors ligne |
| M4 | InstallManager + Library + page Bibliothèque + vérif hash | 🟡 en cours | — |
| M5 | GameLauncher + ModManager + bouton « Créer le dossier mods » | ⬜ à faire | — |
| M6 | Fiche jeu + WikipediaProvider + attribution | ⬜ à faire | — |
| M7 | Online : serveur local, serveurs publics, LAN, manuel | ⬜ à faire | — |
| M8 | Jeu externe + adaptateurs + test des 4 cas §7 | ⬜ à faire | — |
| M9 | i18n FR/EN, packaging, README, finitions | ⬜ à faire | — |

## Checklist « Terminé » (§12)

- [ ] Les 3 pages (Catalogue, Bibliothèque, Amis) + Paramètres fonctionnent. *(Catalogue
  et navigation/Paramètres de base seulement ; Bibliothèque M4, Amis M7 — case non
  cochée tant que les trois pages ne sont pas fonctionnelles)*
- [ ] Catalogue complet issu des providers, filtrable, utilisable hors ligne.
- [ ] Fiche jeu avec données Wikipédia + attribution.
- [ ] Bouton « Créer le dossier mods » + prise en compte des mods au lancement.
- [ ] Options en ligne (local / public) uniquement quand déclarées.
- [ ] Page Amis : serveurs (LAN, liste, manuel) + carnet d'amis local.
- [ ] Les 4 cas de §7 passent en test d'acceptation.
- [x] Aucune ROM/clé/firmware/asset copyrighté dans le dépôt ni téléchargé.
- [x] README (FR+EN) avec limites connues (Smash Ultimate / Mario Kart 8 Deluxe).
- [ ] CI verte (fmt, clippy, tests, deny, audit), artefacts Windows + Linux.
- [ ] PROGRESS.md à jour, « Prochaine action » = terminé.

> La case « 3 pages + Paramètres » est techniquement partielle : le squelette et la
> navigation existent, mais Bibliothèque/Amis ne sont pas encore fonctionnelles. Elle
> sera cochée à la fin de M7.

## En cours (détail)

- Tâche : **M4** — installation, bibliothèque, hash et extraction sécurisée.
- Fichiers à créer : `crates/core/src/library/{mod,library,install}.rs`,
  `crates/core/src/hash.rs`, `crates/core/src/archive/{mod,extract}.rs`,
  `crates/core/src/net/download.rs`, `crates/app/src/views/library.rs`.
- Ce qui reste : tout le pipeline + UI + tests d'acceptation M4.

## Décisions prises (résumé ; détail dans docs/DECISIONS.md)

- 2026-10-21 — Cible Windows GNU localement — aucun MSVC/Build Tools sur la machine ;
  le code reste portable MSVC (CI).
- 2026-10-21 — Source « recomp » introuvable — fallback ManifestProvider + seed vérifié.
- 2026-10-21 — Validation JSON Schema via `jsonschema` + désérialisation typée stricte.
- 2026-10-21 — SQLite via `spawn_blocking` + mutex non tenu sur `.await`.
- 2026-10-21 — `BaseDirs` + `RecompLauncher` pour obtenir exactement
  `%APPDATA%\RecompLauncher` (ProjectDirs ajoutait un sous-dossier `data`).
- 2026-10-21 — Un seul commit de bootstrap M0–M3 (le dépôt partait de zéro et les
  fichiers se recouvrent) ; M4+ auront un commit par jalon.

## Blocages / questions ouvertes

- URL exacte du site « recomp » non fournie → provider désactivé, manifeste embarqué.
- Aucun binaire réel ne peut être installé sans réseau ; les tests M4 utiliseront
  wiremock + archives factices générées (jamais de contenu de jeu).

## Pièges connus / choses à ne pas refaire

- Ne jamais mettre de ROM/ISO/clé/firmware dans le dépôt, les fixtures ou les tests.
- Les tests réseau doivent utiliser `wiremock`, jamais Internet.
- Sur cette machine, `cargo` doit être appelé avec
  `RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu` et le PATH MinGW
  (`C:\Users\mathi\Qt\Tools\mingw1310_64\bin`), sinon rustup utilise MSVC et échoue
  faute de `link.exe`.
- `Option::is_none_or` est trop récent pour la MSRV 1.80 (clippy `incompatible-msrv`).
- Ne pas utiliser `ProjectDirs::from("", "", ...)` : sous Windows cela crée
  `RecompLauncher\data`.

## Commandes utiles

- Build : `cargo build --workspace`
- Tests : `cargo test --workspace --all-targets`
- Lint : `cargo clippy --workspace --all-targets -- -D warnings`
- Format : `cargo fmt --all -- --check`
- Lancer l'app : `cargo run -p app`

## Journal de sessions (le plus récent en haut)

- Session 1 (2026-10-21) : lecture du spec Rust, création du workspace et des crates,
  M0–M3 (catalogue, validation JSON Schema, HttpClient, cache SQLite, UI virtualisée),
  docs/CI, 43 tests verts, app lancée manuellement. Prochaine étape : M4.
