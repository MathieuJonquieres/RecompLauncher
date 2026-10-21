# PROGRESS — RecompLauncher

_Dernière mise à jour : 2026-10-21 — session n°2_

## ▶ Prochaine action (la plus importante du fichier)

Jalon **M5** — `GameLauncher` + `ModManager`.
1. Créer `crates/core/src/launch/` : résolution de l'exécutable depuis `library_games`
   + `LaunchSpec` du manifeste, arguments, `current_dir`, `std::process::Command`
   (pas de shell), suivi du PID et enregistrement du temps de jeu via
   `Library::record_playtime`.
2. Créer `crates/core/src/mods/` : scan du dossier `mods.dir_name`, détection des
   sous-dossiers/fichiers, activation/désactivation persistée dans la table `mods`,
   et génération des arguments/liens nécessaires au lancement.
3. Ajouter les commandes `Play`, `CreateModsDir`, `ToggleMod`, `RefreshMods` et les
   événements associés dans `backend_bridge.rs`.
4. Ajouter le bouton « Jouer » dans la page Bibliothèque et « Créer le dossier mods »
   dans la fiche jeu. Test d'acceptation : un mod détecté est pris en compte au
   lancement (utiliser un faux exécutable de test, jamais un jeu réel).
5. Passer M5 ✅ avec `cargo test --workspace --all-targets`,
   `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`,
   puis mettre à jour `PROGRESS.md` et commiter M5.

## État de la build

- `cargo build --workspace` : **OK** (Windows GNU local ; CI MSVC/Linux/macOS)
- `cargo test --workspace --all-targets` : **75 passés / 0 échoué / 0 ignoré**
  (75 core, 0 app, 0 testkit)
- `cargo clippy --workspace --all-targets -- -D warnings` : **OK**
- `cargo fmt --all -- --check` : **OK**
- Dernier commit sain : `bec0c08` (M0–M3)
- Travail non commité : M4 (à commiter après cette mise à jour)
- Vérification manuelle : `target/debug/recomp-launcher.exe` démarre, affiche le
  catalogue (6 entrées), la page Bibliothèque et le panneau de détail avec le bouton
  « Installer » ; captures dans `%TEMP%\recomp-launcher-*.png`.

## Jalons (§8 du spec)

| # | Jalon | Statut | Preuve (test / commit / démo) |
|---|---|---|---|
| M0 | Workspace + CI + fenêtre eframe vide + tracing | ✅ | `cargo build --workspace`, `cargo clippy`, lancement manuel de l'exe |
| M1 | Sources de données + schéma JSON + validation | ✅ | `catalog::manifest` : tests schéma/seed/snapshot insta, seed de 6 projets vérifiés |
| M2 | HttpClient, ManifestProvider, CatalogService + cache | ✅ | tests wiremock (ETag, retries, limites), cache hors ligne, migrations v1→v2 |
| M3 | UI Catalogue (virtualisée, filtres, détail) | ✅ | `show_rows` + test 500 entrées, lancement app, détail/panneau, bandeau hors ligne |
| M4 | InstallManager + Library + page Bibliothèque + vérif hash | ✅ | 75 tests core ; install ZIP wiremock, rollback archive malveillante, hash SHA-256, reprise `Range`, extraction sécurisée ZIP/tar/7z, réparation, désinstallation conservant `userdata/`/`mods/` ; UI Bibliothèque + détail vérifiés par captures |
| M5 | GameLauncher + ModManager + bouton « Créer le dossier mods » | 🟡 en cours | — |
| M6 | Fiche jeu + WikipediaProvider + attribution | ⬜ à faire | — |
| M7 | Online : serveur local, serveurs publics, LAN, manuel | ⬜ à faire | — |
| M8 | Jeu externe + adaptateurs + test des 4 cas §7 | ⬜ à faire | — |
| M9 | i18n FR/EN, packaging, README, finitions | ⬜ à faire | — |

## Checklist « Terminé » (§12)

- [ ] Les 3 pages (Catalogue, Bibliothèque, Amis) + Paramètres fonctionnent. *(Catalogue
  et Bibliothèque fonctionnels ; Amis M7 ; Paramètres encore partiels)*
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

## En cours (détail)

- Tâche : **M5** — lancement des jeux et gestion des mods.
- Fichiers à créer : `crates/core/src/launch/`, `crates/core/src/mods/`,
  évolutions de `crates/app/src/backend_bridge.rs` et `views/library.rs`.
- Ce qui reste : tout le pipeline de lancement + scan/activation des mods + tests.

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
- 2026-10-21 — M4 : extraction via `SafeRelPath` + limites taille/entrées/ratio ;
  refus des liens, doublons, chemins absolus, préfixes de lecteur et noms réservés.
- 2026-10-21 — M4 : téléchargement `.part` + `Range`, vérification SHA-256, retries,
  annulation `CancellationToken` ; le fichier final n'apparaît qu'après succès.
- 2026-10-21 — M4 : installation dans un dossier temporaire puis `rename` atomique,
  ancienne version conservée jusqu'au succès, rollback complet en cas d'échec.
- 2026-10-21 — M4 : la désinstallation supprime les fichiers gérés mais **conserve**
  `userdata/` et `mods/` ; les dumps utilisateur ne sont jamais copiés ni supprimés.
- 2026-10-21 — M4 : « Réparer » vérifie d'abord l'existence du dossier/exécutable/
  fichier utilisateur ; réinstalle depuis la dernière release seulement si nécessaire.

## Blocages / questions ouvertes

- URL exacte du site « recomp » non fournie → provider désactivé, manifeste embarqué.
- Aucun binaire réel ne peut être installé sans réseau ; les tests M4 utilisent
  wiremock + archives factices générées (jamais de contenu de jeu).
- Le téléchargement réel dépend d'un token GitHub optionnel pour éviter les limites
  d'API ; l'interface ne permet pas encore de le saisir (M9/Paramètres).

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
- Les archives ZIP générées par `zip` masquent les permissions Unix à `0o777` ; tester
  les liens symboliques avec tar, pas avec le writer ZIP.
- `tar::Header::set_path` refuse `..` : pour tester une traversée, écrire les octets
  bruts du champ nom puis recalculer la checksum.
- `tempfile` est une dépendance normale de `core` (pas seulement dev) pour le dossier
  d'installation temporaire.

## Commandes utiles

- Build : `cargo build --workspace`
- Tests : `cargo test --workspace --all-targets`
- Lint : `cargo clippy --workspace --all-targets -- -D warnings`
- Format : `cargo fmt --all -- --check`
- Lancer l'app : `cargo run -p app`

## Journal de sessions (le plus récent en haut)

- Session 2 (2026-10-21) : M4 terminé — hash SHA-1/SHA-256/MD5/CRC32, extraction
  sécurisée ZIP/tar/tar.gz/tar.xz/7z avec `SafeRelPath`/proptest, downloader
  streaming avec reprise et annulation, `Library` SQLite, pipeline d'installation
  atomique avec rollback, réparation et désinstallation préservant `userdata/`/`mods/`,
  UI Bibliothèque + actions catalogue. 75 tests core verts, clippy/fmt verts, app
  lancée et capturée. Prochaine étape : M5.
- Session 1 (2026-10-21) : lecture du spec Rust, création du workspace et des crates,
  M0–M3 (catalogue, validation JSON Schema, HttpClient, cache SQLite, UI virtualisée),
  docs/CI, 43 tests verts, app lancée manuellement. Prochaine étape : M4.
