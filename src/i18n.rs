//! UI translations with [Fluent](https://projectfluent.org).
//!
//! Messages live in `locales/<lang>/main.ftl` and are embedded at compile time.
//! `t("tool-select")` returns the message for the active language,
//! `tf("status-saving", &[("pending", n.into())])` fills Fluent variables (`{ $pending }`).
//! Missing messages fall back to English, then to the id itself.
//! To add a language: add a [`Lang`] variant and its `.ftl` file in [`SOURCES`].

use std::collections::HashMap;
use std::fmt::{self, Display};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use serde::{Deserialize, Serialize};
use unic_langid::LanguageIdentifier;

pub use fluent_bundle::FluentValue;

/// Language setting; `System` follows the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    System,
    En,
    De,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::System, Lang::En, Lang::De];

    /// The concrete language, resolving `System` (English when unknown).
    pub fn resolve(self) -> Self {
        match self {
            Lang::System => match sys_locale::get_locale() {
                Some(l) if l.to_lowercase().starts_with("de") => Lang::De,
                _ => Lang::En,
            },
            lang => lang,
        }
    }

    fn index(self) -> usize {
        match self.resolve() {
            Lang::De => 1,
            _ => 0,
        }
    }
}

impl Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Lang::System => t("settings-system"),
            Lang::En => "English",
            Lang::De => "Deutsch",
        })
    }
}

/// `(locale, ftl source)` per resolved language, indexed by [`Lang::index`].
const SOURCES: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/main.ftl")),
    ("de", include_str!("../locales/de/main.ftl")),
];

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang.index() as u8, Ordering::Relaxed);
}

fn current() -> usize {
    CURRENT.load(Ordering::Relaxed) as usize
}

/// Translates a message without variables.
pub fn t(id: &'static str) -> &'static str {
    // one leaked string per (language, id): bounded by the size of the .ftl files
    static CACHE: OnceLock<Mutex<HashMap<(usize, &'static str), &'static str>>> = OnceLock::new();
    let lang = current();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    cache
        .entry((lang, id))
        .or_insert_with(|| format(lang, id, None).leak())
}

/// Translates a message and fills its `{ $name }` variables.
pub fn tf(id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    let mut fluent = FluentArgs::new();
    for (name, value) in args {
        fluent.set(*name, value.clone());
    }
    format(current(), id, Some(&fluent))
}

fn bundles() -> &'static [FluentBundle<FluentResource>; 2] {
    static BUNDLES: OnceLock<[FluentBundle<FluentResource>; 2]> = OnceLock::new();
    BUNDLES.get_or_init(|| SOURCES.map(|(locale, ftl)| bundle(locale, ftl)))
}

fn bundle(locale: &str, ftl: &str) -> FluentBundle<FluentResource> {
    let id: LanguageIdentifier = locale.parse().expect("valid locale");
    let resource = FluentResource::try_new(ftl.to_owned()).unwrap_or_else(|(res, _)| res);
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    // no Unicode isolation marks around variables: the UI fonts would show them as boxes
    bundle.set_use_isolating(false);
    let _ = bundle.add_resource(resource);
    bundle
}

fn format(lang: usize, id: &str, args: Option<&FluentArgs>) -> String {
    let bundles = bundles();
    [&bundles[lang], &bundles[0]]
        .into_iter()
        .find_map(|b| {
            let pattern = b.get_message(id)?.value()?;
            Some(
                b.format_pattern(pattern, args, &mut Vec::new())
                    .into_owned(),
            )
        })
        .unwrap_or_else(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn ids(ftl: &str) -> BTreeSet<String> {
        let resource = FluentResource::try_new(ftl.to_owned()).expect("ftl parses");
        resource
            .entries()
            .filter_map(|e| match e {
                fluent_syntax::ast::Entry::Message(m) => Some(m.id.name.to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn languages_have_the_same_messages() {
        let en = ids(SOURCES[0].1);
        for (locale, ftl) in &SOURCES[1..] {
            assert_eq!(ids(ftl), en, "{locale}");
        }
        for (locale, ftl) in SOURCES {
            let mut bundle = FluentBundle::new_concurrent(vec![locale.parse().unwrap()]);
            let resource = FluentResource::try_new(ftl.to_owned()).unwrap();
            assert!(
                bundle.add_resource(resource).is_ok(),
                "duplicate ids in {locale}"
            );
        }
    }

    #[test]
    fn used_ids_exist() {
        let en = ids(SOURCES[0].1);
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap();
            for call in ["t(", "tf("] {
                for (i, _) in src.match_indices(call) {
                    let before = src[..i].chars().next_back();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let rest = src[i + call.len()..].trim_start();
                    let Some(rest) = rest.strip_prefix('"') else {
                        continue;
                    };
                    let id = &rest[..rest.find('"').unwrap()];
                    assert!(en.contains(id), "{}: unknown message {id}", path.display());
                }
            }
        }
    }

    #[test]
    fn formats_variables_and_plurals() {
        let args = |n: usize| {
            let mut a = FluentArgs::new();
            a.set("count", n);
            a.set("number", n);
            a
        };
        assert_eq!(format(0, "default-clan-name", Some(&args(3))), "Clan 3");
        assert_eq!(format(1, "default-clan-name", Some(&args(3))), "Sippe 3");
        assert_eq!(format(0, "clan-member-count", Some(&args(1))), "1 member");
        assert_eq!(
            format(1, "clan-member-count", Some(&args(2))),
            "2 Mitglieder"
        );
        assert_eq!(format(1, "no-such-message", None), "no-such-message");
    }
}
