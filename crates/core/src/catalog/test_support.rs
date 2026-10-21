//! Test-only constructors for catalog types.

use std::collections::BTreeMap;

use super::model::*;

pub fn minimal_entry(slug: &str) -> GameEntry {
    let mut exe = BTreeMap::new();
    exe.insert(Os::Windows, "TestGame.exe".to_string());

    GameEntry {
        schema: 1,
        slug: Slug::new(slug).expect("valid test slug"),
        title: "Test Game".to_string(),
        original_platform: "N64".to_string(),
        kind: GameKind::Recomp,
        status: GameStatus::Playable,
        description: "A test game".to_string(),
        project: Project {
            name: "Test Project".to_string(),
            repo: "https://github.com/example/test-game".to_string(),
            license: Some("MIT".to_string()),
        },
        releases: Releases {
            source: ReleaseSource::Github,
            assets: vec![AssetPattern {
                os: Os::Windows,
                arch: Some(Arch::X86_64),
                pattern: "*Windows.zip".to_string(),
                sha256: None,
            }],
        },
        user_files: Some(UserFiles {
            required: true,
            description: "Your own dump".to_string(),
            accepted_hashes: vec![],
            prompt_ext: vec![".z64".to_string()],
        }),
        launch: LaunchSpec { exe, args: vec![] },
        mods: ModsSpec {
            supported: true,
            dir_name: "mods".to_string(),
            format_hint: Some(".nrm".to_string()),
            strategy: ModStrategy::NativeDir,
            args_template: None,
            docs_url: None,
        },
        online: OnlineSpec::default(),
        metadata: MetadataRefs::default(),
        provider: None,
    }
}

pub fn entry_json(slug: &str) -> serde_json::Value {
    serde_json::to_value(minimal_entry(slug)).expect("serializable test entry")
}
