//! Test-only helpers shared by the workspace crates.
//!
//! Fixtures never contain ROMs, ISOs, keys, firmware or copyrighted assets:
//! only small JSON documents and generated dummy bytes.

#![forbid(unsafe_code)]

use recomp_core::AppPaths;

/// Temporary application directory with the base tree created.
pub struct TempApp {
    _dir: tempfile::TempDir,
    paths: AppPaths,
}

impl TempApp {
    pub fn new() -> std::io::Result<Self> {
        let dir = tempfile::tempdir()?;
        let paths = AppPaths::new(dir.path());
        paths.ensure_base_dirs().map_err(std::io::Error::other)?;
        Ok(Self { _dir: dir, paths })
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }
}

/// A minimal valid manifest used by several unit tests.
pub fn minimal_manifest(slug: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": 1,
        "slug": slug,
        "title": "Test Game",
        "original_platform": "N64",
        "kind": "recomp",
        "status": "playable",
        "project": {
            "name": "Test Project",
            "repo": "https://github.com/example/test-game",
            "license": "MIT"
        },
        "releases": {
            "source": "github",
            "assets": [
                { "os": "windows", "arch": "x86_64", "pattern": "*Windows.zip" }
            ]
        },
        "user_files": {
            "required": true,
            "description": "Your own dump",
            "accepted_hashes": [],
            "prompt_ext": [".z64"]
        },
        "launch": {
            "exe": { "windows": "TestGame.exe" },
            "args": []
        },
        "mods": { "supported": true, "dir_name": "mods", "strategy": "native_dir" },
        "online": { "supported": false, "modes": [] },
        "metadata": { "wikipedia_title": null, "igdb_id": null }
    })
}
