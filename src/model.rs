//! Domain model: people, clans (families), lines (links) and attached files.

use std::collections::HashMap;

use iced::{Color, Rectangle};
use serde::{Deserialize, Serialize};

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

/// Suggested line names shown when a new line is drawn.
pub const LINK_PRESETS: [&str; 8] = [
    "married",
    "parents of",
    "father of",
    "mother of",
    "siblings",
    "partner",
    "godfather of",
    "godmother of",
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
    if [
        "married", "siblings", "sibling", "partner", "divorced", "spouse",
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
    ["parent", "father", "mother"]
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

/// All loaded data plus id -> index maps.
#[derive(Debug, Clone, Default)]
pub struct Data {
    pub people: Vec<Person>,
    pub person_idx: HashMap<Id, usize>,
    pub clans: Vec<Clan>,
    pub clan_idx: HashMap<Id, usize>,
    pub links: Vec<Link>,
    pub files: Vec<FileRec>,
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
