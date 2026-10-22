# RecompLauncher — décisions d'architecture et arbitrages

Ce fichier journalise chaque décision non triviale, avec sa date et sa justification.
Le spec (`AGENT_SPEC_recomp_launcher_rust.md`) reste la source de vérité.

## 2026-10-21 — Cible de compilation locale Windows GNU

La machine de développement ne dispose ni de Visual Studio Build Tools ni d'un linker MSVC.
Le toolchain stable `x86_64-pc-windows-gnu` est utilisé localement (MinGW 13.1 fourni avec Qt).
Le code reste portable MSVC : la CI GitHub Actions cible `windows-latest` (MSVC), `ubuntu-latest`
et `macos-latest`. Aucune API spécifique à un linker n'est utilisée.

## 2026-10-21 — Absence de source « recomp » vérifiable

Le brief mentionne « le site recomp » sans URL. Recherche effectuée le 2026-10-21 :
pas d'API JSON officielle ni de flux RSS/sitemap identifié de façon certaine.
Décision : implémenter `ManifestProvider` (fallback garanti) avec `catalog.seed.json` embarqué,
et laisser `RecompSiteProvider` désactivé tant qu'une URL réelle n'est pas fournie. Le provider
HTML existe derrière un flag de configuration et ne scrape aucun contenu protégé par login.
Détail : `docs/DATA_SOURCES.md`.

## 2026-10-21 — Validation en deux étapes

1. Validation JSON Schema Draft 2020-12 (`jsonschema`) sur chaque entrée.
2. Désérialisation typée (`serde`) vers `GameEntry` avec enums et newtypes.

Les champs inconnus sont ignorés (pas de `deny_unknown_fields`). Une entrée invalide est
loggée (`tracing::warn!`) puis ignorée : elle ne fait jamais échouer tout le catalogue.

## 2026-10-21 — Accès SQLite

`rusqlite` (bundled) est accédé via un unique `Connection` protégé par `std::sync::Mutex` et
toutes les opérations passent par `tokio::task::spawn_blocking`. Le mutex n'est jamais tenu
à travers un `.await`. Ce choix évite un thread dédié supplémentaire tout en respectant
l'interdiction de bloquer la boucle UI. `rusqlite_migration` gère les migrations embarquées.

## 2026-10-21 — Pas de `unwrap()` hors tests

`#![forbid(unsafe_code)]` dans tous les crates. Les erreurs sont typées (`thiserror`) dans
`core`, `anyhow` uniquement aux frontières du binaire.

## 2026-10-21 — i18n Fluent

Les textes UI passent par `fluent-templates` (FR par défaut, EN fourni). Les libellés de
données (titres de jeux, plateformes) ne sont pas traduits.

## 2026-10-21 — Extraction d'archives sécurisée (M4)

Chaque entrée passe par `archive::SafeRelPath`, qui normalise les séparateurs et refuse
`..`, les chemins absolus, les préfixes de lecteur Windows, les noms réservés (`CON`,
`NUL`, `COM1`…), les caractères interdits, les composants finissant par un point/espace et
les doublons. Les liens symboliques/physiques sont refusés (tar, mode Unix ZIP, anti-item
7z). Les limites par défaut sont 8 Gio au total, 4 Gio par fichier, 20 000 entrées et un
ratio de compression de 250:1. L'extraction se fait dans un dossier temporaire vide, jamais
directement dans le dossier final.

## 2026-10-21 — Téléchargement et installation atomique (M4)

Le téléchargement écrit dans `<fichier>.part`, reprend via `Range` après une coupure,
expose la progression et peut être annulé avec `CancellationToken`. Le SHA-256 publié
(manifeste ou digest GitHub) est vérifié avant de renommer `.part` vers le fichier final.
L'installation extrait dans `tempfile`, localise l'exécutable déclaré par le manifeste,
puis déplace le dossier par `rename` atomique. L'ancienne version est conservée jusqu'à
l'écriture réussie en base ; en cas d'échec, elle est restaurée et la nouvelle copie est
supprimée.

## 2026-10-21 — Désinstallation et réparation (M4)

« Réparer » vérifie d'abord le dossier, l'exécutable et le fichier utilisateur ; une
installation saine n'est pas retéléchargée. Sinon, la dernière release est réinstallée avec
le pipeline normal. La désinstallation supprime uniquement les fichiers gérés par le
launcher et **conserve** `userdata/` et `mods/` : ces dossiers peuvent contenir des données
créées par l'utilisateur ou par le jeu. Les dumps personnels ne sont jamais copiés,
téléversés ni supprimés.

## 2026-10-21 — Lancement et mods (M5)

Le lancement utilise `std::process::Command` avec un exécutable résolu depuis
`library_games` et `LaunchSpec`, sans passer par un shell. Le chemin déclaré est validé
par `archive::SafeRelPath` avant d'être joint au dossier d'installation. Le PID est
conservé et le temps de jeu est enregistré via `Library::record_playtime` lorsque le
processus se termine. Les jeux externes utilisent l'exécutable/arguments/dossier de
travail stockés, jamais un contenu de jeu copié.

Les mods activés vivent dans `games/<slug>/<dir_name>/` et les désactivés dans
`games/<slug>/<dir_name>.disabled/`. Le scan ne suit pas les liens symboliques et ne
retient que les fichiers dont l'extension correspond à `format_hint` (les dossiers sont
acceptés). L'import ne conserve que le dernier composant du chemin source et refuse les
liens. La stratégie `native_dir` recopie uniquement les mods activés dans un miroir du
dossier de travail du jeu (manifeste interne pour retirer les anciens miroirs) ; elle ne
touche jamais un dossier choisi manuellement. La stratégie `args` remplace `{path}` et
`{name}` jeton par jeton, ce qui préserve les chemins contenant des espaces. Le launcher
n'exécute jamais le contenu d'un mod.

## 2026-10-21 — Jeu externe

Le type de bibliothèque `external` stocke exécutable + arguments + dossier de travail, sans
aucune connaissance du jeu. Il sert aux titres sans recomp publique (Smash Ultimate,
Mario Kart 8 Deluxe) : le launcher ne fournit ni émulateur, ni clé, ni firmware.

## 2026-10-21 — Métadonnées Wikipédia et cache 30 jours (M6)

`WikipediaProvider` interroge uniquement l'API REST publique
`/api/rest_v1/page/summary/{titre}` via `HttpClient` (User-Agent explicite, allowlist,
taille bornée). La réponse est normalisée en `GameMetadata` et mise en cache dans
`metadata_cache` avec `fetched_at`. Le cache est considéré frais pendant 30 jours ; si le
réseau échoue après expiration, l'ancienne fiche est servie avec un indicateur « cache
expiré » et le manifeste reste toujours affiché. L'attribution « Wikipédia — CC BY-SA 4.0 »
et le lien source sont obligatoires dans l'UI.

## 2026-10-21 — Serveurs, LAN, ping et amis locaux (M7)

Les options en ligne ne sont rendues que si `online.supported = true` dans le manifeste.
`OnlineService` agrège les entrées manuelles SQLite, une éventuelle liste JSON publique
configurée par `online.servers_url` et les résultats mDNS de `_recomp._udp.local.`
(configurable par `online.lan_service`). Les doublons sont éliminés par `host:port`.
Le ping est un `TcpStream::connect` avec timeout de 2 s et au plus 16 connexions
simultanées (`buffer_unordered`). `join_template` accepte `{address}`, `{port}` et
`{endpoint}`. Le carnet d'amis (`friends`, `friend_handles`) est strictement local : aucun
compte, aucune synchronisation et aucun envoi de données.

## 2026-10-21 — Adaptateurs par slug (M8)

`GameAdapter` expose `pre_launch`, `build_args`, `post_install` et
`parse_server_status`, avec des implémentations par défaut neutres. `AdapterRegistry` est
la seule association slug → logique spéciale ; `GameLauncher` consulte le registre après
la résolution du manifeste et des mods. Aucun `if slug == ...` n'est dispersé dans le
code. Les quatre cas du §7 sont couverts par `core::acceptance` avec des exécutables
factices et des fichiers utilisateur factices, jamais du contenu de jeu.

## 2026-10-21 — i18n, paramètres et packaging (M9)

Les ressources Fluent sont dans `resources/i18n/{fr-FR,en-US}` avec repli `en-US` ;
`Locale::Fr` est le défaut. Le choix de langue est persisté dans `ui.language` et appliqué
immédiatement. La navigation, le catalogue (barre de filtres), la bibliothèque, la page
Amis et les Paramètres sont localisés ; les fiches détaillées et quelques libellés
dynamiques restent en français et seront migrés vers les mêmes clés. La page
Paramètres expose langue, thème, proxy, URLs de manifeste/liste de serveurs, cache, export
des logs et dossier de données. Le workflow `release.yml` construit des archives Windows,
Linux et macOS sur tag `v*` ; les archives ne contiennent que le binaire et le README.
La refonte UI (M10) remplace les vues localisées par des vues françaises dédiées ; le
mécanisme Fluent du core reste intact mais n'est plus le chemin d'affichage principal.

## 2026-10-21 — Refonte de l'interface egui (M10)

Le binaire reste Rust + egui/eframe : aucune migration React/Tauri/webview. Le design
1440×900 est implémenté avec une fenêtre sans décoration, une palette et des tokens
sombre/clair, Inter (400/500/600/700) et Lucide embarqués. La navigation se limite à
Catalogue, Bibliothèque et Paramètres, plus la fiche jeu et les modales d'installation
et de désinstallation. Les pages Amis et le bloc compte/avatar sont retirés ; les
commandes de jeu externe sont retirées de la surface de l'app.

Les données affichées sont réelles : catalogue, bibliothèque, installs, releases,
mods, serveurs et statistiques de stockage proviennent du core. Aucune jaquette, aucun
emoji et aucun jeu de démonstration ne sont ajoutés. Les cartes dont l'OS courant n'est
pas pris en charge sont voilées, mais restent sélectionnables pour consulter la fiche ;
leurs boutons sont désactivés. Les squelettes de chargement apparaissent après 1 s et
la vérification de fichiers de l'installation reste visible au moins 1,2 s.

egui ne fournit pas de backdrop blur : le voile des modales est plus opaque pour
compenser. Les vues de la refonte sont en français ; le sélecteur de langue affiche
« Français » à titre informatif et l'i18n Fluent du core n'est pas modifiée. Le
redimensionnement est géré par des poignées invisibles et la fenêtre reste utilisable
à 1180×720, avec un catalogue à deux colonnes et un panneau de détail de 310 px.

## 2026-10-21 — Limites assumées (mise à jour M10)

- L'URL du site « recomp » reste inconnue : `RecompSiteProvider` n'est pas activé.
- Le catalogue seed ne contient aucun build macOS sur cette machine : le parcours réel
  d'installation complet n'a pas été exercé de bout en bout. Les tests core couvrent le
  téléchargement, les hashes, la reprise, l'extraction, le rollback et la désinstallation
  avec des archives générées ; les modales sont branchées sur les commandes réelles et
  vérifiées visuellement.
- Les clics synthétiques macOS n'atteignent pas la fenêtre malgré l'autorisation
  d'événements ; les captures ont utilisé un harnais d'environnement temporaire, retiré
  du code final.
- Cinq avertissements `epaint::text::font` au démarrage (glyphe de remplacement absent
  de Lucide) sont sans impact visuel.
