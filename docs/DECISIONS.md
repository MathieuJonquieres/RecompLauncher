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

## 2026-10-21 — Jeu externe

Le type de bibliothèque `external` stocke exécutable + arguments + dossier de travail, sans
aucune connaissance du jeu. Il sert aux titres sans recomp publique (Smash Ultimate,
Mario Kart 8 Deluxe) : le launcher ne fournit ni émulateur, ni clé, ni firmware.
