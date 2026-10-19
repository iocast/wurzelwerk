//! Domain model: people, clans (families), lines (links) and attached files.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use iced::{Color, Rectangle};
use serde::{Deserialize, Serialize};

use crate::i18n::{self, t};

pub type Id = String;

pub fn new_id() -> Id {
    uuid::Uuid::new_v4().to_string()
}

/// Node size in world units.
pub const NODE_W: f32 = 120.0;
pub const NODE_H: f32 = 46.0;

/// Clan colours (teal, amber, …) - cycled for new clans.
pub const PALETTE: [[f32; 3]; 10] = [
    [0.184, 0.478, 0.451],
    [0.663, 0.451, 0.137],
    [0.294, 0.392, 0.627],
    [0.659, 0.271, 0.271],
    [0.408, 0.533, 0.247],
    [0.553, 0.341, 0.596],
    [0.200, 0.545, 0.659],
    [0.733, 0.396, 0.204],
    [0.400, 0.400, 0.400],
    [0.616, 0.494, 0.137],
];

/// Message ids of the suggested line names shown when a new line is drawn.
pub const LINK_PRESETS: [&str; 8] = [
    "link-married",
    "link-parents-of",
    "link-father-of",
    "link-mother-of",
    "link-siblings",
    "link-partner",
    "link-godfather-of",
    "link-godmother-of",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub born: String,
    #[serde(default)]
    pub died: String,
    pub x: f32,
    pub y: f32,
    /// A person can belong to several clans.
    #[serde(default)]
    pub clans: Vec<Id>,
}

impl Person {
    pub fn years(&self) -> String {
        match (year_of(&self.born), year_of(&self.died)) {
            (Some(b), Some(d)) => format!("{b}–{d}"),
            (Some(b), None) => b,
            (None, Some(d)) => format!("–{d}"),
            (None, None) => String::new(),
        }
    }

    /// Where the person stands in `year` (timeline). Without a birth year the person
    /// cannot be placed in time and counts as living until a known death year.
    pub fn era(&self, year: i32) -> Era {
        if year_num(&self.born).is_some_and(|b| year < b) {
            Era::Unborn
        } else if year_num(&self.died).is_some_and(|d| year >= d) {
            Era::Dead
        } else {
            Era::Living
        }
    }

    /// Alive at some point in `from..=to` (timeline range). Unknown dates never hide anyone.
    pub fn lived_within(&self, from: i32, to: i32) -> bool {
        year_num(&self.born).is_none_or(|b| b <= to)
            && year_num(&self.died).is_none_or(|d| d >= from)
    }
}

/// A person's state in a given year of the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Era {
    Unborn,
    Living,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clan {
    pub id: Id,
    pub name: String,
    pub color: [f32; 3],
    #[serde(default)]
    pub label_offset: [f32; 2],
    /// Top-left corner of the box drawn while the clan has no members.
    #[serde(default)]
    pub position: Option<[f32; 2]>,
}

/// Size of the box of a clan without members, in world units.
pub const CLAN_BOX_W: f32 = 170.0;
pub const CLAN_BOX_H: f32 = 70.0;

impl Clan {
    pub fn color(&self) -> Color {
        Color::from_rgb(self.color[0], self.color[1], self.color[2])
    }

    /// Board area of the clan's box (shown only while it has no members).
    pub fn box_rect(&self) -> Option<Rectangle> {
        self.position.map(|[x, y]| Rectangle {
            x,
            y,
            width: CLAN_BOX_W,
            height: CLAN_BOX_H,
        })
    }
}

/// A named line between two people ("married", "parents of", "godfather of", …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub id: Id,
    pub from: Id,
    pub to: Id,
    pub label: String,
    #[serde(default)]
    pub route: Vec<[f32; 2]>,
    #[serde(default)]
    pub start_port: Option<u8>,
    #[serde(default)]
    pub end_port: Option<u8>,
    #[serde(default)]
    pub label_position: Option<LinkLabelPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LinkLabelPosition {
    pub segment: usize,
    pub along: f32,
    pub offset: [f32; 2],
    #[serde(skip)]
    pub legacy_absolute: bool,
}

impl<'de> Deserialize<'de> for LinkLabelPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Attached {
                segment: usize,
                along: f32,
                offset: [f32; 2],
            },
            LegacyAbsolute([f32; 2]),
        }

        Ok(match Stored::deserialize(deserializer)? {
            Stored::Attached {
                segment,
                along,
                offset,
            } => Self {
                segment,
                along,
                offset,
                legacy_absolute: false,
            },
            Stored::LegacyAbsolute(position) => Self {
                segment: 0,
                along: 0.0,
                offset: position,
                legacy_absolute: true,
            },
        })
    }
}

/// A line name the user typed ("affair of"), offered again as a suggestion and
/// translated per language in the workspace settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub id: Id,
    /// Name per locale ("en", "de"); an empty or missing one falls back to another language.
    #[serde(default)]
    pub names: BTreeMap<String, String>,
}

impl Relation {
    /// A relation named in the active language.
    pub fn new(name: &str) -> Self {
        Self {
            id: new_id(),
            names: BTreeMap::from([(i18n::locale().to_owned(), name.trim().to_owned())]),
        }
    }

    /// The name in the active language, else in any other.
    pub fn name(&self) -> &str {
        let filled = |n: &&String| !n.trim().is_empty();
        self.names
            .get(i18n::locale())
            .filter(filled)
            .or_else(|| self.names.values().find(filled))
            .map_or("", |n| n.trim())
    }
}

/// What follows `name` in a trimmed `label` ("married 1789" -> " 1789"), if the label
/// starts with it as a whole word (ASCII case ignored).
fn strip_name<'a>(label: &'a str, name: &str) -> Option<&'a str> {
    let name = name.trim();
    let rest = label.get(name.len()..)?;
    (!name.is_empty()
        && label[..name.len()].eq_ignore_ascii_case(name)
        && (rest.is_empty() || rest.starts_with(' ')))
    .then_some(rest)
}

/// File attached to a person. The file itself lives in `<project>/person/<person-id>/files/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileRec {
    pub id: Id,
    pub person: Id,
    pub name: String,
    /// Path relative to the project folder, `/`-separated.
    pub rel_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStyle {
    /// married, siblings, partner - plain line, no arrow
    Undirected,
    /// parents of, father of, … - line with arrow
    Directed,
    /// everything else (godfather of, friend of, …) - dashed line with arrow
    Dashed,
}

pub fn link_style(label: &str) -> LinkStyle {
    let l = label.trim().to_lowercase();
    // labels are free text in the UI language, and files may move between languages
    if [
        "married",
        "siblings",
        "sibling",
        "partner",
        "divorced",
        "spouse",
        "verheiratet",
        "geschwister",
        "geschieden",
        "ehepartner",
        "ehefrau",
        "ehemann",
    ]
    .iter()
    .any(|k| l.starts_with(k))
    {
        LinkStyle::Undirected
    } else if is_parent_label(&l) {
        LinkStyle::Directed
    } else {
        LinkStyle::Dashed
    }
}

pub fn is_parent_label(label: &str) -> bool {
    let l = label.trim().to_lowercase();
    ["parent", "father", "mother", "eltern", "vater", "mutter"]
        .iter()
        .any(|k| l.starts_with(k))
}

/// Last 4-digit group in a free-text date ("12 May 1989" -> "1989").
pub fn year_of(s: &str) -> Option<String> {
    let mut best = None;
    let mut cur = String::new();
    for ch in s.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else {
            if cur.len() == 4 {
                best = Some(cur.clone());
            }
            cur.clear();
        }
    }
    best
}

/// [`year_of`] as a number, without allocating (called per card on every redraw).
pub fn year_num(s: &str) -> Option<i32> {
    let mut best = None;
    let (mut value, mut digits) = (0i32, 0);
    for b in s.bytes().chain(std::iter::once(b' ')) {
        if b.is_ascii_digit() {
            // long digit runs wrap; only runs of exactly four digits are kept
            value = value.wrapping_mul(10).wrapping_add(i32::from(b - b'0'));
            digits += 1;
        } else {
            if digits == 4 {
                best = Some(value);
            }
            (value, digits) = (0, 0);
        }
    }
    best
}

/// All loaded data plus id -> index maps.
#[derive(Debug, Clone, Default)]
pub struct Data {
    pub people: Vec<Person>,
    pub person_idx: HashMap<Id, usize>,
    pub clans: Vec<Clan>,
    pub clan_idx: HashMap<Id, usize>,
    pub links: Vec<Link>,
    pub files: Vec<FileRec>,
    pub relations: Vec<Relation>,
}

impl Data {
    /// Builds the container; duplicate ids (last write wins) are collapsed.
    pub fn from_parts(
        people: Vec<Person>,
        clans: Vec<Clan>,
        links: Vec<Link>,
        files: Vec<FileRec>,
    ) -> Self {
        Self {
            people: dedup(people, |p| &p.id),
            clans: dedup(clans, |c| &c.id),
            links: dedup(links, |l| &l.id),
            files: dedup(files, |f| &f.id),
            ..Self::default()
        }
        .reindexed()
    }

    fn reindexed(mut self) -> Self {
        self.reindex();
        self
    }

    pub fn reindex(&mut self) {
        self.person_idx = self
            .people
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
        self.reindex_clans();
    }

    pub fn reindex_clans(&mut self) {
        self.clan_idx = self
            .clans
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.clone(), i))
            .collect();
    }

    /// The suggested or user-added line name `label` starts with, in the active language,
    /// and the rest of the label ("Married 1789" -> ("verheiratet", " 1789") in German).
    pub fn known_name<'s, 'a>(&'s self, label: &'a str) -> Option<(&'s str, &'a str)> {
        let label = label.trim();
        let presets = i18n::preset_names()
            .iter()
            .flat_map(|(id, names)| names.iter().map(|n| (n.as_str(), t(id))));
        let custom = self
            .relations
            .iter()
            .flat_map(|r| r.names.values().map(|n| (n.as_str(), r.name())));
        presets
            .chain(custom)
            .filter_map(|(name, shown)| Some((name.trim().len(), shown, strip_name(label, name)?)))
            .max_by_key(|&(len, ..)| len)
            .map(|(_, shown, rest)| (shown, rest))
    }

    /// A line name as shown in the active language; unknown names stay as typed.
    pub fn link_label<'a>(&self, label: &'a str) -> Cow<'a, str> {
        match self.known_name(label) {
            Some((name, rest)) if label.trim() != format!("{name}{rest}") => {
                Cow::Owned(format!("{name}{rest}"))
            }
            _ => Cow::Borrowed(label),
        }
    }

    /// Keeps a typed line name as a [`Relation`] unless it is already known.
    pub fn remember_relation(&mut self, label: &str) -> Option<&Relation> {
        if label.trim().is_empty() || self.known_name(label).is_some() {
            return None;
        }
        self.relations.push(Relation::new(label));
        self.relations.last()
    }

    /// Renames a relation in one language and the lines that used the old name;
    /// returns the changed lines.
    pub fn rename_relation(&mut self, id: &str, locale: &str, name: String) -> Vec<Id> {
        let Some(r) = self.relations.iter_mut().find(|r| r.id == id) else {
            return Vec::new();
        };
        let old = r.names.insert(locale.to_owned(), name).unwrap_or_default();
        let new = match r.names[locale].trim() {
            "" => r.name().to_owned(),
            name => name.to_owned(),
        };
        if old.trim().is_empty() || new.is_empty() {
            return Vec::new();
        }
        let mut changed = Vec::new();
        for link in &mut self.links {
            if let Some(rest) = strip_name(link.label.trim(), &old) {
                link.label = format!("{new}{rest}");
                changed.push(link.id.clone());
            }
        }
        changed
    }

    pub fn person(&self, id: &str) -> Option<&Person> {
        self.person_idx.get(id).map(|&i| &self.people[i])
    }

    pub fn person_mut(&mut self, id: &str) -> Option<&mut Person> {
        let i = *self.person_idx.get(id)?;
        self.people.get_mut(i)
    }

    pub fn push_person(&mut self, p: Person) -> usize {
        let i = self.people.len();
        self.person_idx.insert(p.id.clone(), i);
        self.people.push(p);
        i
    }

    /// O(1) removal (swap_remove). Indices of the moved element are fixed up.
    pub fn remove_person(&mut self, id: &str) -> Option<Person> {
        let i = self.person_idx.remove(id)?;
        let p = self.people.swap_remove(i);
        if i < self.people.len() {
            let moved = self.people[i].id.clone();
            self.person_idx.insert(moved, i);
        }
        Some(p)
    }

    pub fn clan(&self, id: &str) -> Option<&Clan> {
        self.clan_idx.get(id).map(|&i| &self.clans[i])
    }

    pub fn clan_mut(&mut self, id: &str) -> Option<&mut Clan> {
        let i = *self.clan_idx.get(id)?;
        self.clans.get_mut(i)
    }
}

fn dedup<T>(items: Vec<T>, id: impl Fn(&T) -> &Id) -> Vec<T> {
    let mut seen: HashMap<Id, usize> = HashMap::with_capacity(items.len());
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        let key = id(&item).clone();
        match seen.get(&key) {
            Some(&i) => out[i] = item,
            None => {
                seen.insert(key, out.len());
                out.push(item);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    fn link(label: &str) -> Link {
        Link {
            id: new_id(),
            from: "a".into(),
            to: "b".into(),
            label: label.into(),
            route: Vec::new(),
            start_port: None,
            end_port: None,
            label_position: None,
        }
    }

    // the only test that switches the language
    #[test]
    fn relations_are_remembered_shown_translated_and_renamed() {
        let mut data = Data::default();
        i18n::set(Lang::De);
        assert_eq!(data.link_label("mother of"), "Mutter von");
        assert_eq!(data.link_label("Married 1789"), "verheiratet 1789");
        assert_eq!(data.link_label("Vater von"), "Vater von");
        assert_eq!(data.link_label("motherly"), "motherly");
        assert!(data.remember_relation("Mutter von").is_none());

        let id = data.remember_relation("Affäre").unwrap().id.clone();
        assert!(data.remember_relation("Affäre 1850").is_none());
        data.links = vec![link("Affäre 1850"), link("Affären")];
        let changed = data.rename_relation(&id, "en", "affair".into());
        assert!(changed.is_empty());

        i18n::set(Lang::En);
        assert_eq!(data.link_label("Taufpatin von"), "godmother of");
        assert_eq!(data.link_label("Affäre 1850"), "affair 1850");
        assert_eq!(data.link_label("Affären"), "Affären");

        let changed = data.rename_relation(&id, "de", "Liebschaft".into());
        assert_eq!(changed, vec![data.links[0].id.clone()]);
        assert_eq!(data.links[0].label, "Liebschaft 1850");
        assert_eq!(data.link_label("Liebschaft 1850"), "affair 1850");
    }
}
