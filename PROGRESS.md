# PROGRESS — RecompLauncher

_Dernière mise à jour : 2026-10-21 — session n°5 (refonte de l'interface)_

## ▶ Prochaine action (la plus importante du fichier)

**Refonte de l'interface livrée — aucun jalon bloquant à reprendre.** Les seules
limites restantes sont documentées ci-dessous (URL du site « recomp » inconnue,
installation réelle non exerçable sur cette machine faute de build macOS dans le
catalogue, avertissements de police Lucide sans impact visuel).

## État de la build

- `cargo build --workspace` : **OK** (macOS local ; CI Windows/Linux/macOS)
- `cargo test --workspace --all-targets` : **104 passés / 0 échoué / 0 ignoré**
  (102 core + 2 app)
- `cargo clippy --workspace --all-targets -- -D warnings` : **OK**
- `cargo fmt --all -- --check` : **OK**
- Dernier commit sain : `21a4183` (M6–M9) ; refonte UI dans le commit courant
- Vérification manuelle : fenêtre 1440×900 et 1180×720, catalogue (3 colonnes et
  2 colonnes + panneau 310 px), bibliothèque vide, paramètres, fiche jeu, modales
  d'installation et de désinstallation, thèmes sombre et clair.

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
| M7 | Online : serveur local, serveurs publics, LAN, manuel | ✅ | `online` : `ServerStore` SQLite, `JsonServerProvider` (wiremock), mDNS `_recomp._udp.local.`, ajout manuel `host:port`, ping TCP 2 s / 16 parallèles, `join_template` ; le carnet local reste dans le core mais n'est plus exposé dans l'UI. |
| M8 | Jeu externe + adaptateurs + test des 4 cas §7 | ✅ | Sélecteur de fichier `rfd` + ajout manuel (retiré de la surface de commandes de l'app lors de la refonte), lancement externe sans entrée catalogue, `GameAdapter`/`AdapterRegistry` (pre_launch, build_args, post_install, parse_server_status), 4 tests `core::acceptance` (N64 recomp + mods, port decomp + fichier utilisateur, natif sans fichier, jeu externe) |
| M9 | i18n FR/EN, packaging, README, finitions | ✅ | Fluent `resources/i18n/{fr-FR,en-US}`, locale FR par défaut + repli EN, 2 tests ; préférences persistées en SQLite ; workflow release `v*` Windows/Linux/macOS ; README FR+EN et DECISIONS à jour. La refonte UI hardcode le français dans les nouvelles vues ; le sélecteur de langue est informatif. |
| M10 | Refonte UI egui (design 1440×900) | ✅ | Fenêtre sans décoration, palette et tokens sombre/clair, Inter 400/500/600/700 + Lucide, navigation Catalogue/Bibliothèque/Paramètres, fiche jeu à onglets, panneau de détail 360/310 px, modales d'installation en 4 étapes et de désinstallation, vues vides et squelettes ; captures à 1440×900 et 1180×720, thèmes sombre et clair. |

## Checklist « Terminé » (§12)

- [x] Les pages Catalogue, Bibliothèque, Paramètres + la fiche jeu fonctionnent.
- [x] Catalogue complet issu des providers, filtrable, utilisable hors ligne.
- [x] Fiche jeu avec données Wikipédia + attribution.
- [x] Bouton « Créer le dossier mods » + prise en compte des mods au lancement
  (scan, activation/désactivation, ordre, import, miroir `native_dir`, arguments `args`).
- [x] Options en ligne (local / public) uniquement quand déclarées, dans l'onglet
  « En ligne » de la fiche jeu.
- [x] Les 4 cas de §7 passent en test d'acceptation.
- [x] Aucune ROM/clé/firmware/asset copyrighté dans le dépôt ni téléchargé.
- [x] README (FR+EN) avec limites connues (Smash Ultimate / Mario Kart 8 Deluxe).
- [x] Refonte UI : French-first, thème sombre/clair, responsive 1440×900 / 1180×720,
  aucune page Amis ni bloc de compte, aucune jaquette inventée.
- [x] CI configurée (fmt, clippy, tests, cargo-deny, cargo-audit) et workflows de build
  Windows/Linux + release Windows/Linux/macOS ; vérifications locales fmt/clippy/tests vertes.
- [x] PROGRESS.md à jour, « Prochaine action » = terminé.

## En cours (détail)

- Aucun jalon en cours. Finition optionnelle : masquer les cinq avertissements
  `epaint::text::font` du démarrage (glyphe de remplacement Lucide), sans impact UI.

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
- 2026-10-21 — M10 : refonte UI dans egui, pas de migration web. Palette/tokens
  sombre+clair, Inter et Lucide embarqués, fenêtre sans décoration avec poignées de
  redimensionnement invisibles. Le flou d'arrière-plan est approximé par un voile
  opaque (egui n'expose pas de backdrop blur).
- 2026-10-21 — M10 : données réelles uniquement. Les cartes indisponibles restent
  sélectionnables pour consulter la fiche, mais leurs actions sont désactivées. Les
  modales n'offrent que la dernière release réellement publiée.
- 2026-10-21 — M10 : page Amis et bloc compte/avatar retirés de l'UI. `FriendsStore`
  reste dans le core (aucune suppression de données), mais plus aucune commande ni vue
  ne l'expose. Les commandes de jeu externe sont retirées de la surface de l'app.
- 2026-10-21 — M10 : installation guidée en 4 étapes (Version, Vérification,
  Configuration, Terminé) branchée sur la progression/le hash réels ; affichage de la
  vérification maintenu au moins 1,2 s ; squelettes de chargement après 1 s.
- 2026-10-21 — M10 : les nouvelles vues sont en français ; l'i18n Fluent du core reste
  intacte et le sélecteur de langue est informatif (« Français »).

## Blocages / questions ouvertes

- URL exacte du site « recomp » non fournie → provider désactivé, manifeste embarqué.
- Le catalogue seed ne contient aucun build macOS sur cette machine : le parcours
  réel installation → vérification → configuration → désinstallation n'a pas pu être
  exercé de bout en bout. Les tests core couvrent téléchargement, hash, reprise,
  extraction, rollback et désinstallation avec des archives générées ; les modales
  ont été vérifiées visuellement avec les données réelles du catalogue.
- Le jeton GitHub optionnel n'est pas encore exposé dans la page Paramètres ; les
  téléchargements publics fonctionnent sans lui (limite d'API GitHub possible).
- Les clics synthétiques macOS n'atteignent pas la fenêtre malgré l'autorisation
  d'événements ; les captures ont utilisé un harnais d'environnement temporaire,
  retiré du code final.

## Pièges connus / choses à ne pas refaire

- Ne jamais mettre de ROM/ISO/clé/firmware dans le dépôt, les fixtures ou les tests.
- Les tests réseau doivent utiliser `wiremock`, jamais Internet.
- Sur cette machine, `cargo` doit être appelé avec
  `RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu` et le PATH MinGW
  (`C:\Users\mathi\Qt\Tools\mingw1310_64\bin`), sinon rustup utilise MSVC et échoue
  faute de `link.exe`. (Les vérifications de cette session ont été faites sur macOS.)
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
- Egui : ne pas appeler `theme::current(ctx)` à l'intérieur de `ctx.style_mut`, ni
  `ctx.load_texture` à l'intérieur de `ctx.data_mut` (interblocage au démarrage).
- Egui : un `ui.scope_builder` positionné en haut du hero déplace le curseur parent
  vers le bas de l'overlay, donc en arrière ; restaurer avec
  `ui.advance_cursor_after_rect(hero)`.
- Thème clair : peindre le voile « indisponible » avant les textes/badges, sinon les
  titres deviennent blancs sur blanc.
- Le formatage des chemins doit abréger `$HOME` en `~/…` et conserver les deux
  extrémités lors d'une troncature.

## Commandes utiles

- Build : `cargo build --workspace`
- Tests : `cargo test --workspace --all-targets`
- Lint : `cargo clippy --workspace --all-targets -- -D warnings`
- Format : `cargo fmt --all -- --check`
- Lancer l'app : `cargo run -p app`

## Journal de sessions (le plus récent en haut)

- Session 5 (2026-10-21) : refonte UI complète du binaire egui — shell sans décoration,
  palette/tokens sombre+clair, Inter/Lucide, navigation Catalogue/Bibliothèque/
  Paramètres, fiche jeu (Vue d'ensemble, Mods, En ligne, Configuration), catalogue
  responsive avec panneau 360/310 px, modales d'installation 4 étapes et de
  désinstallation, squelettes et vues vides. Nettoyage des commandes Amis et jeu
  externe de la surface UI, ajout du filtrage par OS, des statistiques de stockage et
  du diagnostic. 104 tests verts, clippy/fmt verts, captures 1440×900 et 1180×720 en
  sombre et clair. Projet prêt à livrer.
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
