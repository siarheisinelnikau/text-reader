//! Explicit UI catalogs. Document text, voice names and paths are never translated.
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Russian,
    French,
    German,
    Spanish,
    Italian,
    #[default]
    #[serde(other)]
    English,
}

static CATALOG: LazyLock<HashMap<String, [String; 6]>> = LazyLock::new(|| {
    let rows: Vec<[String; 6]> = serde_json::from_str(include_str!("locales.json"))
        .expect("the embedded UI catalog is valid");
    rows.into_iter().map(|row| (row[0].clone(), row)).collect()
});

impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Russian,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
    ];
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
        }
    }
    pub fn text(self, key: &'static str) -> &'static str {
        CATALOG.get(key).map_or(key, |row| {
            row[match self {
                Self::English => 0,
                Self::Russian => 1,
                Self::French => 2,
                Self::German => 3,
                Self::Spanish => 4,
                Self::Italian => 5,
            }]
            .as_str()
        })
    }
    pub fn format(self, key: &'static str, args: &[(&str, String)]) -> String {
        let mut template = self.text(key);
        let mut result = String::with_capacity(template.len());
        while let Some(start) = template.find('{') {
            result.push_str(&template[..start]);
            let Some(end) = template[start..].find('}').map(|end| start + end) else {
                result.push_str(&template[start..]);
                return result;
            };
            let name = &template[start + 1..end];
            result.push_str(
                args.iter()
                    .find(|(key, _)| *key == name)
                    .map_or(&template[start..=end], |(_, value)| value.as_str()),
            );
            template = &template[end + 1..];
        }
        result.push_str(template);
        result
    }
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::English),
            "ru" => Some(Self::Russian),
            "fr" => Some(Self::French),
            "de" => Some(Self::German),
            "es" => Some(Self::Spanish),
            "it" => Some(Self::Italian),
            _ => None,
        }
    }
}

#[derive(Clone, Default)]
pub struct Message {
    key: &'static str,
    args: Vec<(&'static str, String)>,
}
impl From<&'static str> for Message {
    fn from(key: &'static str) -> Self {
        Self { key, args: vec![] }
    }
}
impl Message {
    pub fn with(key: &'static str, args: &[(&'static str, String)]) -> Self {
        Self {
            key,
            args: args.to_vec(),
        }
    }
    pub fn error(details: String) -> Self {
        Self::with(
            "The operation could not be completed. Details: {details}",
            &[("details", details)],
        )
    }
    pub fn render(&self, language: Language) -> String {
        language.format(self.key, &self.args)
    }
    pub fn is_empty(&self) -> bool {
        self.key.is_empty()
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn placeholders(text: &str) -> Vec<&str> {
        let mut names: Vec<_> = text
            .split('{')
            .skip(1)
            .map(|part| part.split('}').next().unwrap())
            .collect();
        names.sort_unstable();
        names
    }
    #[test]
    fn every_catalog_entry_has_all_languages_and_matching_parameters() {
        let rows: Vec<[String; 6]> = serde_json::from_str(include_str!("locales.json")).unwrap();
        assert_eq!(rows.len(), CATALOG.len(), "duplicate source messages");
        for row in rows {
            for translation in &row {
                assert!(!translation.trim().is_empty());
                assert_eq!(
                    placeholders(&row[0]),
                    placeholders(translation),
                    "{}",
                    row[0]
                );
            }
        }
    }
    #[test]
    fn message_language_can_change_without_altering_arguments() {
        let message = Message::with("Restored {title}", &[("title", "My документ.txt".into())]);
        assert_eq!(
            message.render(Language::English),
            "Restored My документ.txt"
        );
        assert_eq!(
            message.render(Language::Russian),
            "Восстановлен документ My документ.txt"
        );
        assert_eq!(
            Language::French.text("Unknown future message"),
            "Unknown future message"
        );
        assert_eq!(
            Language::English.format(
                "Loaded {title} — {count} passages",
                &[("title", "{count}.txt".into()), ("count", "7".into())]
            ),
            "Loaded {count}.txt — 7 passages"
        );
    }
}
