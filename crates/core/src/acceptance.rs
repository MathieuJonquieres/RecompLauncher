//! End-to-end acceptance scenarios from the specification (§7).
//!
//! Every fixture is synthetic: empty executable files and tiny dummy mod
//! files. No ROM, ISO, key, firmware or copyrighted asset is used.

use std::sync::Arc;

use tempfile::TempDir;

use crate::adapters::{AdapterContext, AdapterRegistry, GameAdapter};
use crate::catalog::model::Os;
use crate::catalog::test_support::minimal_entry;
use crate::db::Db;
use crate::launch::GameLauncher;
use crate::library::Library;
use crate::mods::ModManager;
use crate::paths::AppPaths;

struct TestAdapter;

impl GameAdapter for TestAdapter {
    fn build_args(&self, _context: &AdapterContext<'_>) -> Vec<String> {
        vec!["--adapter".to_string()]
    }
}

struct Fixture {
    _dir: TempDir,
    library: Library,
    mods: ModManager,
    install_dir: std::path::PathBuf,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths::new(dir.path());
    paths.ensure_base_dirs().expect("paths");
    let db = Db::open_in_memory().await.expect("db");
    let library = Library::new(db);
    let mods = ModManager::new(paths, library.clone());
    let install_dir = dir.path().join("game");
    std::fs::create_dir_all(&install_dir).expect("install dir");
    std::fs::write(install_dir.join("TestGame.exe"), b"fake").expect("fake exe");
    Fixture {
        _dir: dir,
        library,
        mods,
        install_dir,
    }
}

#[tokio::test]
async fn n64_recomp_with_user_file_and_mods() {
    let fixture = fixture().await;
    let mut entry = minimal_entry("n64-recomp");
    entry
        .launch
        .exe
        .insert(Os::current(), "TestGame.exe".to_string());
    let user_file = fixture._dir.path().join("user.z64");
    std::fs::write(&user_file, b"user-owned dummy").expect("user file");
    fixture
        .library
        .upsert_managed(
            "n64-recomp",
            "N64 Recomp",
            &fixture.install_dir,
            "1.0.0",
            Some(&user_file),
        )
        .await
        .expect("library");

    let source_mod = fixture._dir.path().join("example.nrm");
    std::fs::write(&source_mod, b"dummy mod").expect("mod");
    fixture
        .mods
        .import_path("n64-recomp", &entry.mods, &source_mod)
        .await
        .expect("import mod");

    let game = fixture
        .library
        .get("n64-recomp")
        .await
        .expect("get")
        .expect("game");
    let mut adapters = AdapterRegistry::new();
    adapters.register("n64-recomp", Arc::new(TestAdapter));
    let launcher =
        GameLauncher::new(fixture.library.clone(), fixture.mods.clone()).with_adapters(adapters);
    let plan = launcher.resolve(&entry, &game).await.expect("resolve");
    assert_eq!(plan.executable, fixture.install_dir.join("TestGame.exe"));
    assert_eq!(plan.mods.enabled, vec!["example.nrm".to_string()]);
    assert!(plan
        .mods
        .native_dir
        .as_ref()
        .is_some_and(|dir| dir.is_dir()));
    assert!(plan.args.contains(&"--adapter".to_string()));
}

#[tokio::test]
async fn decomp_port_with_user_file_without_mods() {
    let fixture = fixture().await;
    let mut entry = minimal_entry("decomp-port");
    entry.kind = crate::catalog::model::GameKind::Decomp;
    entry.mods.supported = false;
    entry
        .launch
        .exe
        .insert(Os::current(), "TestGame.exe".to_string());
    let user_file = fixture._dir.path().join("user.z64");
    std::fs::write(&user_file, b"user-owned dummy").expect("user file");
    fixture
        .library
        .upsert_managed(
            "decomp-port",
            "Decomp Port",
            &fixture.install_dir,
            "1.0.0",
            Some(&user_file),
        )
        .await
        .expect("library");
    let game = fixture
        .library
        .get("decomp-port")
        .await
        .expect("get")
        .expect("game");
    let launcher = GameLauncher::new(fixture.library.clone(), fixture.mods.clone());
    let plan = launcher.resolve(&entry, &game).await.expect("resolve");
    assert!(plan.mods.enabled.is_empty());
    assert_eq!(plan.executable, fixture.install_dir.join("TestGame.exe"));
}

#[tokio::test]
async fn native_game_without_user_file() {
    let fixture = fixture().await;
    let mut entry = minimal_entry("native-game");
    entry.kind = crate::catalog::model::GameKind::NativePort;
    entry.user_files = None;
    entry.mods.supported = false;
    entry
        .launch
        .exe
        .insert(Os::current(), "TestGame.exe".to_string());
    fixture
        .library
        .upsert_managed(
            "native-game",
            "Native Game",
            &fixture.install_dir,
            "1.0.0",
            None,
        )
        .await
        .expect("library");
    let game = fixture
        .library
        .get("native-game")
        .await
        .expect("get")
        .expect("game");
    let launcher = GameLauncher::new(fixture.library.clone(), fixture.mods.clone());
    let plan = launcher.resolve(&entry, &game).await.expect("resolve");
    assert_eq!(plan.executable, fixture.install_dir.join("TestGame.exe"));
}

#[tokio::test]
async fn external_game_is_launched_without_catalogue_entry() {
    let fixture = fixture().await;
    let external = fixture._dir.path().join("external.exe");
    std::fs::write(&external, b"fake").expect("fake exe");
    fixture
        .library
        .add_external(
            "external-game",
            "External Game",
            &external,
            Some("--demo \"two words\""),
            Some(fixture._dir.path()),
        )
        .await
        .expect("external");
    let game = fixture
        .library
        .get("external-game")
        .await
        .expect("get")
        .expect("game");
    let launcher = GameLauncher::new(fixture.library.clone(), fixture.mods.clone());
    let plan = launcher.resolve_external(&game).expect("resolve");
    assert_eq!(plan.executable, external);
    assert_eq!(
        plan.args,
        vec!["--demo".to_string(), "two words".to_string()]
    );
    assert_eq!(plan.cwd, fixture._dir.path());
}
