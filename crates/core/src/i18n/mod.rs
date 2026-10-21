//! Fluent-based internationalisation. French is the default locale and
//! English is bundled as the fallback.
//!
//! UI code never formats user-visible labels by hand for translated keys:
//! it asks [`I18n`] for a message. Untranslated technical identifiers (slugs,
//! paths, error payloads from the backend) remain as-is.

use std::str::FromStr;

use fluent_templates::{static_loader, Loader};
use serde::{Deserialize, Serialize};
use unic_langid::LanguageIdentifier;

static_loader! {
    static LOCALES = {
        locales: "../../resources/i18n",
        fallback_language: "en-US",
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    #[default]
    Fr,
    En,
}

impl Locale {
    pub const ALL: [Locale; 2] = [Locale::Fr, Locale::En];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Fr => "fr-FR",
            Self::En => "en-US",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Fr => "Français",
            Self::En => "English",
        }
    }

    pub fn from_code(code: &str) -> Self {
        if code.to_ascii_lowercase().starts_with("en") {
            Self::En
        } else {
            Self::Fr
        }
    }

    fn language_id(self) -> LanguageIdentifier {
        LanguageIdentifier::from_str(self.code()).unwrap_or_else(|_| {
            LanguageIdentifier::from_str("en-US")
                .unwrap_or_else(|_| LanguageIdentifier::from_str("en").unwrap_or_default())
        })
    }
}

/// Holds the active locale and exposes Fluent lookups.
#[derive(Debug, Clone)]
pub struct I18n {
    locale: Locale,
}

impl Default for I18n {
    fn default() -> Self {
        Self { locale: Locale::Fr }
    }
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        Self { locale }
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = locale;
    }

    /// Returns the translated message, or `fallback` when the key is absent.
    pub fn text_or(&self, key: &str, fallback: &'static str) -> String {
        LOCALES
            .try_lookup(&self.locale.language_id(), key)
            .unwrap_or_else(|| fallback.to_string())
    }

    /// Returns the translated message, or the key itself when absent.
    pub fn text(&self, key: &str) -> String {
        LOCALES
            .try_lookup(&self.locale.language_id(), key)
            .unwrap_or_else(|| key.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_is_default_and_english_is_available() {
        let mut i18n = I18n::default();
        assert_eq!(i18n.locale(), Locale::Fr);
        assert_eq!(i18n.text("nav-catalog"), "Catalogue");
        i18n.set_locale(Locale::En);
        assert_eq!(i18n.text("nav-catalog"), "Catalog");
        assert_eq!(i18n.text("library-title"), "Library");
        assert_eq!(i18n.text("friends-title"), "Friends");
        assert_eq!(i18n.text("catalog-search"), "Search a game, a platform…");
    }

    #[test]
    fn locale_codes_roundtrip() {
        assert_eq!(Locale::from_code("fr-FR"), Locale::Fr);
        assert_eq!(Locale::from_code("en-US"), Locale::En);
        assert_eq!(Locale::from_code("de-DE"), Locale::Fr);
    }
}
