# PROGRESS — RecompLauncher

_Dernière mise à jour : 2026-10-21 — session n°4 (M6–M9)_

## ▶ Prochaine action (la plus importante du fichier)

**Projet terminé — aucun jalon bloquant à reprendre.** Les seules limites restantes
sont documentées ci-dessous (URL du site « recomp » inconnue, pas de test réseau réel,
traduction des vues Catalogue/Bibliothèque/Amis au-delà de la navigation et des
Paramètres).

## État de la build

- `cargo build --workspace` : **OK** (Windows GNU local ; CI MSVC/Linux/macOS)
- `cargo test --workspace --all-targets` : **100 passés / 0 échoué / 0 ignoré**
  (100 core, 0 app, 0 testkit)
- `cargo clippy --workspace --all-targets -- -D warnings` : **OK**
- `cargo fmt --all -- --check` : **OK**
- Dernier commit sain : `eaa272e` (M5 + docs)
- Travail M6–M9 : commit final de cette session
- Vérification manuelle : application démarrée en mode portable, page Paramètres
  fonctionnelle (langue FR/EN, thème, URLs, cache, export des logs, dossier de données),
  page Amis avec carnet local, page Bibliothèque avec jeu externe.

## Jalons (§8 du spec)

| # | Jalon | Statut | Preuve (test / commit / démo) |
|---|---|---|---|
| M0 | Workspace + CI + fenêtre eframe vide + tracing | ✅ | `cargo build --workspace`, `cargo clippy`, lancement manuel de l'exe |
| M1 | Sources de données + schéma JSON + validation | ✅ | `catalog::manifest` : tests schéma/seed/snapshot insta, seed de 6 projets vérifiés |
| M2 | HttpClient, ManifestProvider, CatalogService + cache | ✅ | tests wiremock (ETag, retries, limites), cache hors ligne, migrations v1→v2 |
| M3 | UI Catalogue (virtualisée, filtres, détail) | ✅ | `show_rows` + test 500 entrées, lancement app, détail/panneau, bandeau hors ligne |
| M4 | InstallManager + Library + page Bibliothèque + vérif hash | ✅ | 75 tests core ; install ZIP wiremock, rollback archive malveillante, hash SHA-256, reprise `Range`, extraction sécurisée ZIP/tar/7z, réparation, désinstallation conservant `userdata/`/`mods/` ; UI Bibliothèque + détail vérifiés par captures |
| M5 | GameLauncher + ModManager + bouton « Créer le dossier mods » | ✅ | 85 tests core ; résolution manifeste/OS, `std::process::Command` sans shell, PID + temps de jeu, scan/toggle/ordre/import, miroir `native_dir`, arguments `args`, faux exécutable recevant un mod détecté, UI Bibliothèque vérifiée par capture |
| M6 | Fiche jeu + WikipediaProvider + attribution | ✅ | `metadata` : API REST Wikipédia, cache SQLite `metadata_cache` (TTL 30 j), repli cache expiré, attribution CC BY-SA 4.0 et lien source, 3 tests ; UI détail avec description/liens/source et repli manifeste |
| M7 | Online : serveur local, serveurs publics, LAN, manuel | ✅ | `online` : `ServerStore` SQLite, `JsonServerProvider` (wiremock), mDNS `_recomp._udp.local.`, ajout manuel `host:port`, ping TCP 2 s / 16 parallèles, `join_template`, carnet d'amis local ; page Amis fonctionnelle |
| M8 | Jeu externe + adaptateurs + test des 4 cas §7 | ✅ | Sélecteur de fichier `rfd` + ajout manuel, lancement externe sans entrée catalogue, `GameAdapter`/`AdapterRegistry` (pre_launch, build_args, post_install, parse_server_status), 4 tests `core::acceptance` (N64 recomp + mods, port decomp + fichier utilisateur, natif sans fichier, jeu externe) |
| M9 | i18n FR/EN, packaging, README, finitions | ✅ | Fluent `resources/i18n/{fr-FR,en-US}`, locale FR par défaut + repli EN, 2 tests ; navigation, catalogue, bibliothèque, amis et Paramètres localisés ; page Paramètres (langue, thème, proxy, URLs, cache, logs, dossier de données) ; workflow release `v*` Windows/Linux/macOS ; README FR+EN et DECISIONS à jour. Limite : fiches détaillées encore partiellement en français. |

## Checklist « Terminé » (§12)

- [x] Les 3 pages (Catalogue, Bibliothèque, Amis) + Paramètres fonctionnent.
- [x] Catalogue complet issu des providers, filtrable, utilisable hors ligne.
- [x] Fiche jeu avec données Wikipédia + attribution.
- [x] Bouton « Créer le dossier mods » + prise en compte des mods au lancement
  (scan, activation/désactivation, ordre, import, miroir `native_dir`, arguments `args`).
- [x] Options en ligne (local / public) uniquement quand déclarées.
- [x] Page Amis : serveurs (LAN, liste, manuel) + carnet d'amis local.
- [x] Les 4 cas de §7 passent en test d'acceptation.
- [x] Aucune ROM/clé/firmware/asset copyrighté dans le dépôt ni téléchargé.
- [x] README (FR+EN) avec limites connues (Smash Ultimate / Mario Kart 8 Deluxe).
- [x] CI verte (fmt, clippy, tests, deny, audit), artefacts Windows + Linux.
- [x] PROGRESS.md à jour, « Prochaine action » = terminé.

## En cours (détail)

- Aucun jalon en cours. Finition optionnelle : traduire les libellés internes des
  fiches détaillées (les clés Fluent et le mécanisme sont en place ; navigation,
  catalogue, bibliothèque, amis et Paramètres sont déjà FR/EN).

## Décisions prises (résumé ; détail dans docs/DECISIONS.md)

- 2026-10-21 — Cible Windows GNU localement — aucun MSVC/Build Tools sur la machine ;
  le code reste portable MSVC (CI).
- 2026-10-21 — Source « recomp » introuvable — fallback ManifestProvider + seed vérifié.
- 2026-10-21 — Validation JSON Schema via `jsonschema` + désérialisation typée stricte.
- 2026-10-21 — SQLite via `spawn_blocking` + mutex non tenu sur `.await`.
- 2026-10-21 — `BaseDirs` + `RecompLauncher` pour obtenir exactement
  `%APPDATA%\RecompLauncher` (ProjectDirs ajoutait un sous-dossier `data`).
- 2026-10-21 — Un seul commit de bootstrap M0–M3 ; un commit par jalon ensuite.
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
- 2026-10-21 — M5 : lancement direct avec `std::process::Command` (jamais de shell),
  chemin d'exécutable validé par `SafeRelPath`, PID exposé, temps de jeu enregistré
  après la fin du processus.
- 2026-10-21 — M5 : les mods activés vivent dans `games/<slug>/<dir_name>/`, les mods
  désactivés dans `games/<slug>/<dir_name>.disabled/`. La stratégie `native_dir`
  recopie uniquement les mods activés dans un miroir du dossier de travail ; elle ne
  modifie jamais un dossier choisi manuellement. La stratégie `args` substitue
  `{path}`/`{name}` jeton par jeton.
- 2026-10-21 — M5 : l'import ne prend que le dernier composant du chemin source et
  refuse les liens symboliques ; le launcher n'exécute jamais le contenu d'un mod.
- 2026-10-21 — M6 : Wikipédia via l'API REST publique, cache 30 jours, repli sur cache
  expiré, attribution CC BY-SA 4.0 obligatoire, description du manifeste toujours visible.
- 2026-10-21 — M7 : options en ligne conditionnées par `online.supported`, sources
  manuelles + JSON + mDNS fusionnées et dédupliquées, ping TCP 2 s / 16 max, carnet
  d'amis strictement local (aucun compte ni envoi).
- 2026-10-21 — M8 : toute logique spéciale passe par `AdapterRegistry` (aucun
  `if slug == ...`), jeux externes stockés par chemin d'exécutable seulement.
- 2026-10-21 — M9 : Fluent `fr-FR`/`en-US` (repli `en-US`), FR par défaut, préférences
  persistées en SQLite, thème appliqué immédiatement, release CI sur tags `v*`.

## Blocages / questions ouvertes

- URL exacte du site « recomp » non fournie → provider désactivé, manifeste embarqué.
- Aucun binaire réel ne peut être installé sans réseau ; les tests M4/M7 utilisent
  wiremock + archives/fixtures générées (jamais de contenu de jeu).
- Le jeton GitHub optionnel n'est pas encore exposé dans la page Paramètres ; les
  téléchargements publics fonctionnent sans lui (limite d'API GitHub possible).
- La traduction complète des fiches détaillées reste une finition ; navigation,
  catalogue, bibliothèque, amis et Paramètres sont FR/EN.

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
- Les fichiers `.ftl` ne doivent pas avoir de BOM (sinon `ParserError`) et les clés
  Fluent ne peuvent pas contenir de points ; utiliser des tirets.

## Commandes utiles

- Build : `cargo build --workspace`
- Tests : `cargo test --workspace --all-targets`
- Lint : `cargo clippy --workspace --all-targets -- -D warnings`
- Format : `cargo fmt --all -- --check`
- Lancer l'app : `cargo run -p app`

## Journal de sessions (le plus récent en haut)

- Session 4 (2026-10-21) : M6–M9 terminés — Wikipédia + cache 30 j, serveurs
  JSON/LAN/manuel + ping + amis locaux, jeux externes + adaptateurs + 4 tests §7,
  i18n Fluent FR/EN, page Paramètres, workflow release, README/DECISIONS. 100 tests
  core verts, clippy/fmt verts. Projet terminé.
- Session 3 (2026-10-21) : M5 terminé — `GameLauncher` (résolution manifeste/OS,
  `std::process::Command` sans shell, PID, temps de jeu), `ModManager` (scan,
  activation/désactivation `.disabled`, ordre, import sécurisé, miroir `native_dir`,
  arguments `args`), commandes/événements backend et UI Bibliothèque/détail. Test
  d'acceptation avec faux exécutable recevant le chemin d'un mod détecté. 85 tests
  core verts, clippy/fmt verts, capture Bibliothèque. Prochaine étape : M6.
- Session 2 (2026-10-21) : M4 terminé — hash SHA-1/SHA-256/MD5/CRC32, extraction
  sécurisée ZIP/tar/tar.gz/tar.xz/7z avec `SafeRelPath`/proptest, downloader
  streaming avec reprise et annulation, `Library` SQLite, pipeline d'installation
  atomique avec rollback, réparation et désinstallation préservant `userdata/`/`mods/`,
  UI Bibliothèque + actions catalogue. 75 tests core verts, clippy/fmt verts, app
  lancée et capturée. Prochaine étape : M5.
- Session 1 (2026-10-21) : lecture du spec Rust, création du workspace et des crates,
  M0–M3 (catalogue, validation JSON Schema, HttpClient, cache SQLite, UI virtualisée),
  docs/CI, 43 tests verts, app lancée manuellement. Prochaine étape : M4.
