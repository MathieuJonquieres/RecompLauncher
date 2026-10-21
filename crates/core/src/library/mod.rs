//! Library persistence and installation pipeline.

pub mod install;

use std::path::{Path, PathBuf};

use rusqlite::{params, Row};

use crate::db::{now_unix, Db, DbError};

/// How a game entered the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallSource {
    /// Downloaded and installed by the launcher.
    Managed,
    /// Added from a local folder/exe chosen by the user.
    Manual,
    /// Declared as an external game (no launcher-managed files).
    External,
}

impl InstallSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Manual => "manual",
            Self::External => "external",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Managed => "Installé par le launcher",
            Self::Manual => "Ajouté manuellement",
            Self::External => "Jeu externe",
        }
    }

    fn from_db(value: &str) -> rusqlite::Result<Self> {
        match value {
            "managed" => Ok(Self::Managed),
            "manual" => Ok(Self::Manual),
            "external" => Ok(Self::External),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown library source `{other}`"),
                )),
            )),
        }
    }
}

/// One row of `library_games`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryGame {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub install_path: Option<PathBuf>,
    pub version: Option<String>,
    pub installed_at: i64,
    pub source: InstallSource,
    pub last_played: Option<i64>,
    pub playtime_s: u64,
    pub external_exe: Option<PathBuf>,
    pub external_args: Option<String>,
    pub external_cwd: Option<PathBuf>,
    /// User-owned dump path. The file itself is never copied or uploaded.
    pub user_file: Option<PathBuf>,
}

impl LibraryGame {
    pub fn install_dir(&self) -> Option<&Path> {
        self.install_path.as_deref()
    }

    pub fn executable_hint(&self) -> Option<&Path> {
        self.external_exe
            .as_deref()
            .or(self.install_path.as_deref())
    }
}

#[derive(Clone)]
pub struct Library {
    db: Db,
}

impl Library {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<LibraryGame>, DbError> {
        self.db
            .with_conn(|conn| {
                let mut statement = conn.prepare(
                    "SELECT id, slug, title, install_path, version, installed_at, source, \
                     last_played, playtime_s, external_exe, external_args, external_cwd, user_file \
                     FROM library_games ORDER BY title COLLATE NOCASE, slug",
                )?;
                let rows = statement.query_map([], row_to_game)?;
                rows.collect()
            })
            .await
    }

    pub async fn get(&self, slug: &str) -> Result<Option<LibraryGame>, DbError> {
        let slug = slug.to_string();
        self.db
            .with_conn(move |conn| {
                let mut statement = conn.prepare(
                    "SELECT id, slug, title, install_path, version, installed_at, source, \
                     last_played, playtime_s, external_exe, external_args, external_cwd, user_file \
                     FROM library_games WHERE slug = ?1",
                )?;
                let mut rows = statement.query_map([slug], row_to_game)?;
                rows.next().transpose()
            })
            .await
    }

    /// Inserts or replaces a launcher-managed installation.
    pub async fn upsert_managed(
        &self,
        slug: &str,
        title: &str,
        install_path: &Path,
        version: &str,
        user_file: Option<&Path>,
    ) -> Result<LibraryGame, DbError> {
        let slug = slug.to_string();
        let title = title.to_string();
        let install_path = path_to_string(install_path);
        let version = version.to_string();
        let user_file = user_file.map(path_to_string);
        let installed_at = now_unix();
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO library_games \
                     (slug, title, install_path, version, installed_at, source, user_file) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 'managed', ?6) \
                     ON CONFLICT(slug) DO UPDATE SET \
                       title = excluded.title, \
                       install_path = excluded.install_path, \
                       version = excluded.version, \
                       installed_at = excluded.installed_at, \
                       source = 'managed', \
                       user_file = excluded.user_file",
                    params![slug, title, install_path, version, installed_at, user_file],
                )?;
                query_game(conn, &slug)
            })
            .await
    }

    pub async fn add_manual(
        &self,
        slug: &str,
        title: &str,
        install_path: &Path,
        user_file: Option<&Path>,
    ) -> Result<LibraryGame, DbError> {
        let slug = slug.to_string();
        let title = title.to_string();
        let install_path = path_to_string(install_path);
        let user_file = user_file.map(path_to_string);
        let installed_at = now_unix();
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO library_games \
                     (slug, title, install_path, installed_at, source, user_file) \
                     VALUES (?1, ?2, ?3, ?4, 'manual', ?5) \
                     ON CONFLICT(slug) DO UPDATE SET \
                       title = excluded.title, \
                       install_path = excluded.install_path, \
                       installed_at = excluded.installed_at, \
                       source = 'manual', \
                       user_file = excluded.user_file",
                    params![slug, title, install_path, installed_at, user_file],
                )?;
                query_game(conn, &slug)
            })
            .await
    }

    pub async fn add_external(
        &self,
        slug: &str,
        title: &str,
        executable: &Path,
        args: Option<&str>,
        cwd: Option<&Path>,
    ) -> Result<LibraryGame, DbError> {
        let slug = slug.to_string();
        let title = title.to_string();
        let executable = path_to_string(executable);
        let args = args.map(str::to_string);
        let cwd = cwd.map(path_to_string);
        let installed_at = now_unix();
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO library_games \
                     (slug, title, installed_at, source, external_exe, external_args, external_cwd) \
                     VALUES (?1, ?2, ?3, 'external', ?4, ?5, ?6) \
                     ON CONFLICT(slug) DO UPDATE SET \
                       title = excluded.title, \
                       installed_at = excluded.installed_at, \
                       source = 'external', \
                       external_exe = excluded.external_exe, \
                       external_args = excluded.external_args, \
                       external_cwd = excluded.external_cwd",
                    params![slug, title, installed_at, executable, args, cwd],
                )?;
                query_game(conn, &slug)
            })
            .await
    }

    pub async fn set_user_file(&self, slug: &str, path: Option<&Path>) -> Result<(), DbError> {
        let slug = slug.to_string();
        let path = path.map(path_to_string);
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "UPDATE library_games SET user_file = ?2 WHERE slug = ?1",
                    params![slug, path],
                )?;
                Ok(())
            })
            .await
    }

    /// Records a launch timestamp and adds elapsed playtime.
    pub async fn record_playtime(&self, slug: &str, seconds: u64) -> Result<(), DbError> {
        let slug = slug.to_string();
        let seconds = seconds.min(i64::MAX as u64) as i64;
        let now = now_unix();
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "UPDATE library_games \
                     SET last_played = ?2, playtime_s = playtime_s + ?3 WHERE slug = ?1",
                    params![slug, now, seconds],
                )?;
                Ok(())
            })
            .await
    }

    /// Removes the database row. Files are removed by the caller, never here.
    pub async fn remove(&self, slug: &str) -> Result<bool, DbError> {
        let slug = slug.to_string();
        self.db
            .with_conn(move |conn| {
                let changed = conn.execute("DELETE FROM library_games WHERE slug = ?1", [slug])?;
                Ok(changed > 0)
            })
            .await
    }
}

fn query_game(conn: &rusqlite::Connection, slug: &str) -> rusqlite::Result<LibraryGame> {
    conn.query_row(
        "SELECT id, slug, title, install_path, version, installed_at, source, last_played, \
         playtime_s, external_exe, external_args, external_cwd, user_file \
         FROM library_games WHERE slug = ?1",
        [slug],
        row_to_game,
    )
}

fn row_to_game(row: &Row<'_>) -> rusqlite::Result<LibraryGame> {
    let source: String = row.get(6)?;
    let playtime_s: i64 = row.get(8)?;
    Ok(LibraryGame {
        id: row.get(0)?,
        slug: row.get(1)?,
        title: row.get(2)?,
        install_path: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
        version: row.get(4)?,
        installed_at: row.get(5)?,
        source: InstallSource::from_db(&source)?,
        last_played: row.get(7)?,
        playtime_s: playtime_s.max(0) as u64,
        external_exe: row.get::<_, Option<String>>(9)?.map(PathBuf::from),
        external_args: row.get(10)?,
        external_cwd: row.get::<_, Option<String>>(11)?.map(PathBuf::from),
        user_file: row.get::<_, Option<String>>(12)?.map(PathBuf::from),
    })
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn library() -> Library {
        Library::new(Db::open_in_memory().await.expect("db"))
    }

    #[tokio::test]
    async fn managed_upsert_and_list() {
        let library = library().await;
        let path = Path::new("C:/games/zelda");
        let first = library
            .upsert_managed("zelda64-recomp", "Zelda 64 Recomp", path, "v1.0.0", None)
            .await
            .expect("upsert");
        assert_eq!(first.source, InstallSource::Managed);
        assert_eq!(first.version.as_deref(), Some("v1.0.0"));

        let second = library
            .upsert_managed("zelda64-recomp", "Zelda 64 Recomp", path, "v1.1.0", None)
            .await
            .expect("upsert");
        assert_eq!(second.id, first.id);
        assert_eq!(second.version.as_deref(), Some("v1.1.0"));

        let games = library.list().await.expect("list");
        assert_eq!(games.len(), 1);
    }

    #[tokio::test]
    async fn manual_and_external_entries_are_distinct() {
        let library = library().await;
        library
            .add_manual("manual-game", "Manual", Path::new("C:/manual"), None)
            .await
            .expect("manual");
        library
            .add_external(
                "external-game",
                "External",
                Path::new("C:/external/game.exe"),
                Some("--windowed"),
                Some(Path::new("C:/external")),
            )
            .await
            .expect("external");

        let games = library.list().await.expect("list");
        assert_eq!(games.len(), 2);
        let external = library
            .get("external-game")
            .await
            .expect("get")
            .expect("row");
        assert_eq!(external.source, InstallSource::External);
        assert_eq!(external.external_args.as_deref(), Some("--windowed"));
        assert_eq!(external.install_path, None);
    }

    #[tokio::test]
    async fn user_file_is_only_a_path() {
        let library = library().await;
        library
            .upsert_managed(
                "game",
                "Game",
                Path::new("C:/games/game"),
                "v1",
                Some(Path::new("D:/dumps/game.z64")),
            )
            .await
            .expect("upsert");
        let game = library.get("game").await.expect("get").expect("row");
        assert_eq!(
            game.user_file.as_deref(),
            Some(Path::new("D:/dumps/game.z64"))
        );

        library.set_user_file("game", None).await.expect("clear");
        let game = library.get("game").await.expect("get").expect("row");
        assert!(game.user_file.is_none());
    }

    #[tokio::test]
    async fn playtime_and_removal() {
        let library = library().await;
        library
            .upsert_managed("game", "Game", Path::new("C:/games/game"), "v1", None)
            .await
            .expect("upsert");
        library.record_playtime("game", 42).await.expect("playtime");
        let game = library.get("game").await.expect("get").expect("row");
        assert_eq!(game.playtime_s, 42);
        assert!(game.last_played.is_some());
        assert!(library.remove("game").await.expect("remove"));
        assert!(!library.remove("game").await.expect("remove"));
    }
}
