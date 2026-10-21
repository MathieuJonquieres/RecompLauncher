//! Catalog filtering and sorting. Pure functions, no I/O, heavily testable.

use super::model::{GameEntry, GameKind, GameStatus, Os};

/// Sort order offered in the catalog toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CatalogSort {
    #[default]
    Title,
    Platform,
    Status,
}

/// Active catalog filters. `os_only` hides entries without a build for the
/// current OS; otherwise they stay visible (and greyed in the UI).
#[derive(Debug, Clone, Default)]
pub struct CatalogFilter {
    pub search: String,
    pub platform: Option<String>,
    pub kind: Option<GameKind>,
    pub status: Option<GameStatus>,
    pub os_only: bool,
    pub sort: CatalogSort,
}

impl CatalogFilter {
    pub fn matches(&self, entry: &GameEntry) -> bool {
        if self.os_only && !entry.supports_os(Os::current()) {
            return false;
        }
        if let Some(platform) = &self.platform {
            if !entry.original_platform.eq_ignore_ascii_case(platform) {
                return false;
            }
        }
        if let Some(kind) = self.kind {
            if entry.kind != kind {
                return false;
            }
        }
        if let Some(status) = self.status {
            if entry.status != status {
                return false;
            }
        }

        let needle = self.search.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        entry.title.to_lowercase().contains(&needle)
            || entry.original_platform.to_lowercase().contains(&needle)
            || entry.project.name.to_lowercase().contains(&needle)
            || entry.slug.as_str().contains(&needle)
    }

    /// Applies filters, then sorts. The result is a vector of references so
    /// large catalogs are not cloned for every frame.
    pub fn apply<'a>(&self, entries: &'a [GameEntry]) -> Vec<&'a GameEntry> {
        let mut filtered: Vec<&GameEntry> =
            entries.iter().filter(|entry| self.matches(entry)).collect();

        match self.sort {
            CatalogSort::Title => filtered.sort_by(|a, b| {
                a.title
                    .to_lowercase()
                    .cmp(&b.title.to_lowercase())
                    .then_with(|| a.slug.cmp(&b.slug))
            }),
            CatalogSort::Platform => filtered.sort_by(|a, b| {
                a.original_platform
                    .to_lowercase()
                    .cmp(&b.original_platform.to_lowercase())
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            }),
            CatalogSort::Status => filtered.sort_by(|a, b| {
                status_rank(a.status)
                    .cmp(&status_rank(b.status))
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            }),
        }
        filtered
    }

    pub fn platforms(entries: &[GameEntry]) -> Vec<String> {
        let mut platforms: Vec<String> = entries
            .iter()
            .map(|entry| entry.original_platform.clone())
            .collect();
        platforms.sort_by_key(|value| value.to_lowercase());
        platforms.dedup();
        platforms
    }
}

const fn status_rank(status: GameStatus) -> u8 {
    match status {
        GameStatus::Playable => 0,
        GameStatus::Beta => 1,
        GameStatus::Wip => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::minimal_entry;

    fn sample() -> Vec<GameEntry> {
        let mut zelda = minimal_entry("zelda");
        zelda.title = "The Legend of Zelda".to_string();
        zelda.original_platform = "N64".to_string();

        let mut mario = minimal_entry("mario");
        mario.title = "Super Mario 64".to_string();
        mario.original_platform = "N64".to_string();
        mario.kind = GameKind::Decomp;
        mario.status = GameStatus::Beta;

        let mut diablo = minimal_entry("diablo");
        diablo.title = "Diablo".to_string();
        diablo.original_platform = "PC".to_string();
        diablo.kind = GameKind::NativePort;
        vec![zelda, mario, diablo]
    }

    #[test]
    fn search_is_case_insensitive() {
        let filter = CatalogFilter {
            search: "MARIO".to_string(),
            ..CatalogFilter::default()
        };
        let entries = sample();
        let result = filter.apply(&entries);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slug.as_str(), "mario");
    }

    #[test]
    fn filters_combine() {
        let filter = CatalogFilter {
            platform: Some("N64".to_string()),
            kind: Some(GameKind::Decomp),
            ..CatalogFilter::default()
        };
        let entries = sample();
        let result = filter.apply(&entries);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slug.as_str(), "mario");
    }

    #[test]
    fn sorting_by_title() {
        let entries = sample();
        let result = CatalogFilter::default().apply(&entries);
        let titles: Vec<_> = result.iter().map(|entry| entry.title.as_str()).collect();
        assert_eq!(titles, ["Diablo", "Super Mario 64", "The Legend of Zelda"]);
    }

    #[test]
    fn platforms_are_deduplicated() {
        let entries = sample();
        assert_eq!(
            CatalogFilter::platforms(&entries),
            vec!["N64".to_string(), "PC".to_string()]
        );
    }

    #[test]
    fn five_hundred_entries_are_all_kept() {
        let entries: Vec<GameEntry> = (0..500)
            .map(|index| minimal_entry(&format!("demo-game-{index}")))
            .collect();
        let filtered = CatalogFilter::default().apply(&entries);
        assert_eq!(filtered.len(), 500);
        assert_eq!(
            filtered.first().map(|entry| entry.slug.as_str()),
            Some("demo-game-0")
        );
    }
}
