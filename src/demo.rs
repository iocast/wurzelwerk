//! Synthetic families for load testing and the example workspace:
//! `--demo 40x250` = 40 clans × up to 250 people.
//!
//! Every clan starts with a founding couple in the 17th or 18th century and grows
//! generation by generation up to [`PRESENT`]: births and deaths with the child mortality
//! and life expectancy of their time, weddings dated on the line ("married 1789"),
//! widowed parents who marry again, twins, godparents among the aunts and uncles, and a
//! few spouses who also belong to a neighbouring clan. The growth per generation is
//! chosen so that a clan reaches its size around the present day, which keeps the
//! timeline busy across all centuries.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::model::{Clan, Data, Id, Link, PALETTE, Person, new_id, year_num};
use crate::notes::{self, NoteKey};

/// Size of the example workspace (clans × at most people per clan): about 600 people.
pub const EXAMPLE: (usize, usize) = (11, 80);
/// Nobody is born after this year; whoever would die later is still alive.
const PRESENT: i32 = 2025;
/// Average years between two generations.
const GENERATION: i32 = 29;
/// Name of the generated documentation book.
const CHRONICLE: &str = "Chronicle";

const SURNAMES: [&str; 40] = [
    "Weber", "Roth", "Keller", "Meier", "Huber", "Frei", "Brunner", "Baumann", "Gerber", "Widmer",
    "Steiner", "Fischer", "Moser", "Wyss", "Graf", "Bühler", "Zürcher", "Hofer", "Suter", "Kunz",
    "Lehmann", "Marti", "Vogel", "Ammann", "Bachmann", "Egli", "Frey", "Gasser", "Hess", "Imhof",
    "Jenni", "Kälin", "Lüthi", "Müller", "Nägeli", "Oberholzer", "Pfister", "Rüegg", "Schmid",
    "Tanner",
];

/// First names by era: before 1850, 1850-1949, from 1950.
const MALE: [[&str; 12]; 3] = [
    [
        "Johann", "Jakob", "Hans", "Ulrich", "Heinrich", "Kaspar", "Konrad", "Rudolf", "Melchior",
        "Christian", "Josef", "Niklaus",
    ],
    [
        "Ernst", "Walter", "Fritz", "Otto", "Karl", "Paul", "Emil", "Max", "Werner", "Alfred",
        "Hermann", "Hans",
    ],
    [
        "Noah", "Luca", "Leon", "Elias", "David", "Nico", "Marc", "Felix", "Jonas", "Samuel",
        "Lukas", "Daniel",
    ],
];
const FEMALE: [[&str; 12]; 3] = [
    [
        "Anna", "Verena", "Barbara", "Elisabeth", "Magdalena", "Katharina", "Maria", "Regula",
        "Susanna", "Margaretha", "Ursula", "Dorothea",
    ],
    [
        "Rosa", "Frieda", "Martha", "Lina", "Bertha", "Klara", "Emma", "Gertrud", "Hedwig", "Ida",
        "Margrit", "Elsa",
    ],
    [
        "Mia", "Emma", "Lea", "Lina", "Sofia", "Laura", "Sara", "Julia", "Nina", "Anna", "Lara",
        "Elena",
    ],
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    /// `lo..=hi`
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1).max(1) as u64) as i32
    }
    /// One in `n`.
    fn chance(&mut self, n: u64) -> bool {
        self.below(n) == 0
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

fn clan_name(i: usize) -> String {
    let base = SURNAMES[i % SURNAMES.len()];
    match i / SURNAMES.len() {
        0 => base.to_string(),
        n => format!("{base} {}", n + 1),
    }
}

/// Free-text date as people write it: "c. 1690", "1754" or "14 Mar 1789".
fn date(rng: &mut Rng, year: i32) -> String {
    if year < 1760 && rng.chance(3) {
        format!("c. {year}")
    } else if rng.chance(3) {
        year.to_string()
    } else {
        format!("{} {} {year}", rng.range(1, 28), MONTHS[rng.below(12) as usize])
    }
}

/// Year of death, `None` while still alive. `min_age` keeps people alive long enough
/// for what the story needs (founders, people who marry).
fn death(rng: &mut Rng, born: i32, min_age: i32) -> Option<i32> {
    let child_mortality = match born {
        ..1800 => 5,
        1800..1900 => 8,
        _ => 60,
    };
    let age = if rng.chance(child_mortality) {
        rng.range(0, 9)
    } else {
        let expected = (50 + (born - 1600).max(0) / 11).min(84);
        rng.range(expected - 18, expected + 14)
    };
    let died = born + age.max(min_age);
    if died <= PRESENT {
        Some(died)
    } else if PRESENT - born > 98 {
        // nobody is older than a hundred
        Some(born + rng.range(88, 98))
    } else {
        None
    }
}

/// Couples per generation grow by this factor so that `per_clan` people span `generations`.
fn growth(per_clan: usize, generations: i32) -> f64 {
    // a couple adds the spouse and their children: about four people
    let size = |g: f64| 4.2 * (0..=generations).map(|k| g.powi(k)).sum::<f64>();
    let (mut lo, mut hi) = (1.0, 3.0);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if size(mid) < per_clan as f64 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

struct Life {
    born: i32,
    died: Option<i32>,
    female: bool,
    /// Index into `Gen::families` (the children of one couple).
    family: Option<usize>,
}

impl Life {
    fn alive_in(&self, year: i32) -> bool {
        self.born <= year && self.died.is_none_or(|d| d >= year)
    }
    /// Last year this person can take part in anything.
    fn end(&self) -> i32 {
        self.died.unwrap_or(PRESENT)
    }
}

/// A couple: the clan member, the spouse who married in, and the wedding year.
type Couple = (usize, usize, i32);

struct Gen<'a> {
    rng: Rng,
    clans: &'a [Clan],
    people: Vec<Person>,
    lives: Vec<Life>,
    families: Vec<Vec<usize>>,
    links: Vec<Link>,
}

impl Gen<'_> {
    fn add(&mut self, surname: &str, life: Life, clans: Vec<Id>) -> usize {
        let era = match life.born {
            ..1850 => 0,
            1850..1950 => 1,
            _ => 2,
        };
        let first = if life.female {
            self.rng.pick(&FEMALE[era])
        } else {
            self.rng.pick(&MALE[era])
        };
        let born = date(&mut self.rng, life.born);
        let died = life
            .died
            .map(|d| date(&mut self.rng, d))
            .unwrap_or_default();
        self.people.push(Person {
            id: new_id(),
            name: format!("{first} {surname}"),
            born,
            died,
            x: 0.0,
            y: 0.0,
            clans,
        });
        self.lives.push(life);
        self.people.len() - 1
    }

    fn link(&mut self, a: usize, b: usize, label: impl Into<String>) {
        self.links.push(Link {
            id: new_id(),
            from: self.people[a].id.clone(),
            to: self.people[b].id.clone(),
            label: label.into(),
            route: Vec::new(),
            start_port: None,
            end_port: None,
            label_position: None,
        });
    }

    /// Someone from another family marries `partner` in `wedding`.
    fn marry(&mut self, ci: usize, partner: usize, wedding: i32) -> Couple {
        let p = &self.lives[partner];
        let female = !p.female;
        let born = (p.born + self.rng.range(-6, 6)).min(wedding - 17);
        let died = death(&mut self.rng, born, wedding - born + 1);
        let mut clans = vec![self.clans[ci].id.clone()];
        if self.clans.len() > 1 && self.rng.chance(12) {
            clans.push(self.clans[(ci + 1) % self.clans.len()].id.clone());
        }
        let own = SURNAMES[ci % SURNAMES.len()];
        let surname = loop {
            let s = self.rng.pick(&SURNAMES);
            if s != own {
                break s;
            }
        };
        let spouse = self.add(
            surname,
            Life {
                born,
                died,
                female,
                family: None,
            },
            clans,
        );
        self.link(partner, spouse, format!("married {wedding}"));
        (partner, spouse, wedding)
    }

    /// The children of a couple. Returns those who grow up and may marry; a widowed
    /// partner who marries again is queued as a new couple of the same generation.
    fn family(
        &mut self,
        ci: usize,
        (member, spouse, wedding): Couple,
        queue: &mut VecDeque<Couple>,
        room: &mut usize,
    ) -> Vec<usize> {
        let (father, mother) = if self.lives[member].female {
            (spouse, member)
        } else {
            (member, spouse)
        };
        let last = (self.lives[mother].born + 44)
            .min(self.lives[mother].end())
            .min(self.lives[father].end() + 1)
            .min(PRESENT);
        let wanted = if wedding < 1900 {
            self.rng.range(2, 7)
        } else {
            self.rng.range(1, 3)
        } as usize;

        let surname = SURNAMES[ci % SURNAMES.len()];
        let fam = self.families.len();
        self.families.push(Vec::new());
        let mut kids = Vec::new();
        let mut year = wedding + self.rng.range(0, 2);
        while kids.len() < wanted && year <= last && *room > 0 {
            let twins = *room > 1 && self.rng.chance(40);
            for _ in 0..if twins { 2 } else { 1 } {
                let female = self.rng.chance(2);
                let died = death(&mut self.rng, year, 0);
                let kid = self.add(
                    surname,
                    Life {
                        born: year,
                        died,
                        female,
                        family: Some(fam),
                    },
                    vec![self.clans[ci].id.clone()],
                );
                *room -= 1;
                self.link(father, kid, "father of");
                self.link(mother, kid, "mother of");
                self.godparent(member, kid);
                self.families[fam].push(kid);
                kids.push(kid);
            }
            year += self.rng.range(1, 4);
        }
        if kids.len() >= 2 {
            self.link(kids[0], kids[1], "siblings");
        }

        // the clan member, widowed young, marries again and may have more children
        if let Some(widowed) = self.lives[spouse].died
            && widowed < self.lives[member].end()
        {
            let again = widowed + self.rng.range(1, 3);
            let life = &self.lives[member];
            if again <= PRESENT
                && life.alive_in(again)
                && again - life.born < 45
                && *room > 0
                && self.rng.chance(2)
            {
                *room -= 1;
                queue.push_back(self.marry(ci, member, again));
            }
        }

        kids.retain(|&k| {
            let life = &self.lives[k];
            life.born + 20 <= PRESENT && life.died.is_none_or(|d| d > life.born + 22)
        });
        kids
    }

    /// Now and then an aunt or uncle of the child becomes its godparent.
    fn godparent(&mut self, parent: usize, kid: usize) {
        if !self.rng.chance(7) {
            return;
        }
        let Some(fam) = self.lives[parent].family else {
            return;
        };
        let born = self.lives[kid].born;
        let candidates: Vec<usize> = self.families[fam]
            .iter()
            .copied()
            .filter(|&s| s != parent && self.lives[s].alive_in(born) && born - self.lives[s].born >= 16)
            .collect();
        if candidates.is_empty() {
            return;
        }
        let g = candidates[self.rng.below(candidates.len() as u64) as usize];
        let label = if self.lives[g].female {
            "godmother of"
        } else {
            "godfather of"
        };
        self.link(g, kid, label);
    }

    fn clan(&mut self, ci: usize, per_clan: usize) {
        let mut room = per_clan;
        let founded = 1620 + ((ci * 37) % 120) as i32 + self.rng.range(0, 9);
        let surname = SURNAMES[ci % SURNAMES.len()];
        let wedding = founded + self.rng.range(22, 28);
        let died = death(&mut self.rng, founded, wedding - founded + 15);
        let founder = self.add(
            surname,
            Life {
                born: founded,
                died,
                female: false,
                family: None,
            },
            vec![self.clans[ci].id.clone()],
        );
        room -= 1;
        if room == 0 {
            return;
        }
        room -= 1;
        let mut couples = vec![self.marry(ci, founder, wedding)];
        // the founders' wife lives long enough to raise children
        let wife = couples[0].1;
        if self.lives[wife].died.is_some_and(|d| d < wedding + 15) {
            self.lives[wife].died = Some(wedding + 15 + self.rng.range(0, 30));
            let d = self.lives[wife].died.unwrap_or(PRESENT);
            self.people[wife].died = date(&mut self.rng, d);
        }

        let growth = growth(per_clan, (PRESENT - founded) / GENERATION);
        for generation in 1.. {
            if couples.is_empty() || room == 0 {
                break;
            }
            let mut queue = VecDeque::from(couples);
            let mut grown = Vec::new();
            while let Some(couple) = queue.pop_front() {
                if room == 0 {
                    break;
                }
                grown.extend(self.family(ci, couple, &mut queue, &mut room));
            }
            // shuffle, then the first `budget` grown-up children marry
            for i in (1..grown.len()).rev() {
                let j = self.rng.below(i as u64 + 1) as usize;
                grown.swap(i, j);
            }
            let budget = growth.powi(generation).round().max(1.0) as usize;
            couples = Vec::new();
            for kid in grown.into_iter().take(budget) {
                if room == 0 {
                    break;
                }
                let life = &self.lives[kid];
                let wedding = (life.born + self.rng.range(20, 30))
                    .min(life.end() - 1)
                    .min(PRESENT);
                room -= 1;
                couples.push(self.marry(ci, kid, wedding));
            }
        }
    }
}

pub fn generate(clan_count: usize, per_clan: usize) -> (Vec<Clan>, Vec<Person>, Vec<Link>) {
    let clans: Vec<Clan> = (0..clan_count)
        .map(|i| Clan {
            id: new_id(),
            name: clan_name(i),
            color: PALETTE[i % PALETTE.len()],
            label_offset: [0.0, 0.0],
            position: None,
        })
        .collect();

    let mut g = Gen {
        rng: Rng(0x9E37_79B9_7F4A_7C15),
        clans: &clans,
        people: Vec::with_capacity(clan_count * per_clan),
        lives: Vec::with_capacity(clan_count * per_clan),
        families: Vec::new(),
        links: Vec::with_capacity(clan_count * per_clan * 2),
    };
    if per_clan > 0 {
        for ci in 0..clan_count {
            g.clan(ci, per_clan);
        }
    }
    let (people, links) = (g.people, g.links);
    (clans, people, links)
}

// ---------------------------------------------------------------------------
// notes
// ---------------------------------------------------------------------------

/// Markdown written next to generated data: a note per clan, notes for the founders
/// and a "Chronicle" book with one page per century.
pub fn notes(data: &Data) -> Vec<(NoteKey, String)> {
    let year = |s: &str| year_num(s);
    let mut out = Vec::new();

    for clan in &data.clans {
        let members: Vec<&Person> = data
            .people
            .iter()
            .filter(|p| p.clans.contains(&clan.id))
            .collect();
        let Some(founder) = members
            .iter()
            .filter(|p| year(&p.born).is_some())
            .min_by_key(|p| year(&p.born))
        else {
            continue;
        };
        let born: Vec<i32> = members.iter().filter_map(|p| year(&p.born)).collect();
        let living = members.iter().filter(|p| p.died.is_empty()).count();
        let first = born.iter().min().copied().unwrap_or_default();
        let last = born.iter().max().copied().unwrap_or_default();
        out.push((
            NoteKey::Clan(clan.id.clone()),
            format!(
                "# The {name} family\n\n\
                 The line goes back to **{founder}**, born {fborn}.\n\n\
                 | | |\n|---|---|\n\
                 | Members on the board | {count} |\n\
                 | Born between | {first} and {last} |\n\
                 | Still living | {living} |\n\n\
                 *Generated example data.*\n",
                name = clan.name,
                founder = founder.name,
                fborn = founder.born,
                count = members.len(),
            ),
        ));
        out.push((
            NoteKey::Person(founder.id.clone()),
            notes::with_preamble(
                founder,
                &format!(
                    "# {name}\n\nFounder of the {clan} family.\n\n\
                     - Born: {born}\n- Died: {died}\n\n\
                     ## Sources\n\n- Parish register (example)\n- Family bible (example)\n",
                    name = founder.name,
                    clan = clan.name,
                    born = founder.born,
                    died = if founder.died.is_empty() { "–" } else { founder.died.as_str() },
                ),
            ),
        ));
    }

    // chronicle: births, deaths and weddings per century
    #[derive(Default)]
    struct Century<'a> {
        births: usize,
        deaths: usize,
        weddings: usize,
        oldest: Option<(&'a Person, i32)>,
        names: HashMap<&'a str, usize>,
    }
    let mut centuries: BTreeMap<i32, Century> = BTreeMap::new();
    for p in &data.people {
        let (b, d) = (year(&p.born), year(&p.died));
        if let Some(b) = b {
            let c = centuries.entry(b / 100 * 100).or_default();
            c.births += 1;
            if let Some(first) = p.name.split(' ').next() {
                *c.names.entry(first).or_default() += 1;
            }
        }
        if let Some(d) = d {
            let c = centuries.entry(d / 100 * 100).or_default();
            c.deaths += 1;
            if let Some(b) = b
                && c.oldest.is_none_or(|(_, age)| d - b > age)
            {
                c.oldest = Some((p, d - b));
            }
        }
    }
    for link in &data.links {
        if link.label.starts_with("married")
            && let Some(y) = year(&link.label)
        {
            centuries.entry(y / 100 * 100).or_default().weddings += 1;
        }
    }
    for (start, c) in &centuries {
        let mut names: Vec<(&str, usize)> = c.names.iter().map(|(n, k)| (*n, *k)).collect();
        names.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let names = names
            .iter()
            .take(5)
            .map(|(n, k)| format!("{n} ({k})"))
            .collect::<Vec<_>>()
            .join(", ");
        let oldest = c.oldest.map_or_else(
            || "–".to_owned(),
            |(p, age)| format!("{} ({}, {age} years)", p.name, p.years()),
        );
        out.push((
            NoteKey::Page {
                book: CHRONICLE.into(),
                page: format!("{start}–{}", start + 99),
            },
            format!(
                "# {start}–{end}\n\n\
                 | | |\n|---|---|\n\
                 | Births | {births} |\n| Deaths | {deaths} |\n| Weddings | {weddings} |\n\n\
                 **Most common first names:** {names}\n\n\
                 **Longest life ending in this century:** {oldest}\n\n\
                 Use the timeline (T) to watch the families grow through these years.\n",
                end = start + 99,
                births = c.births,
                deaths = c.deaths,
                weddings = c.weddings,
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_span_the_centuries_up_to_the_present() {
        let (clans, people, links) = generate(6, 300);
        assert_eq!(clans.len(), 6);
        assert!(people.len() > 6 * 200, "{} people", people.len());
        assert!(people.len() <= 6 * 300);

        let born: Vec<i32> = people.iter().filter_map(|p| year_num(&p.born)).collect();
        assert_eq!(born.len(), people.len(), "everyone has a birth year");
        assert!(born.iter().all(|&b| (1600..=PRESENT).contains(&b)));
        assert!(*born.iter().min().unwrap() < 1700);
        assert!(*born.iter().max().unwrap() > 1980);
        for p in &people {
            if let (Some(b), Some(d)) = (year_num(&p.born), year_num(&p.died)) {
                assert!(b <= d && d <= PRESENT, "{p:?}");
            }
        }
        assert!(people.iter().any(|p| p.died.is_empty()), "somebody is alive");

        let index: HashMap<&str, &Person> = people.iter().map(|p| (p.id.as_str(), p)).collect();
        for link in &links {
            let (from, to) = (index[link.from.as_str()], index[link.to.as_str()]);
            let (fb, tb) = (year_num(&from.born).unwrap(), year_num(&to.born).unwrap());
            if link.label.ends_with(" of") {
                assert!(fb + 14 <= tb, "{} {} {}", from.name, link.label, to.name);
            }
            if let Some(wedding) = link.label.strip_prefix("married ") {
                let wedding: i32 = wedding.parse().unwrap();
                assert!(wedding >= fb.max(tb) + 14, "{link:?}");
            }
        }
    }

    /// `cargo test --release bench_example -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_example() {
        let t = std::time::Instant::now();
        let (clans, people, links) = generate(EXAMPLE.0, EXAMPLE.1);
        let generated = t.elapsed();
        let mut data = Data::from_parts(people, clans, links, Vec::new());
        crate::layout::auto_layout(&mut data);
        let born: Vec<i32> = data.people.iter().filter_map(|p| year_num(&p.born)).collect();
        println!(
            "{} people, {} lines, born {}-{}, living {}; generate {generated:?}, layout {:?}",
            data.people.len(),
            data.links.len(),
            born.iter().min().unwrap(),
            born.iter().max().unwrap(),
            data.people.iter().filter(|p| p.died.is_empty()).count(),
            t.elapsed() - generated,
        );
    }

    #[test]
    fn notes_cover_clans_founders_and_centuries() {
        let (clans, people, links) = generate(3, 120);
        let data = Data::from_parts(people, clans, links, Vec::new());
        let notes = notes(&data);
        let count = |f: fn(&NoteKey) -> bool| notes.iter().filter(|(k, _)| f(k)).count();
        assert_eq!(count(|k| matches!(k, NoteKey::Clan(_))), 3);
        assert_eq!(count(|k| matches!(k, NoteKey::Person(_))), 3);
        assert!(count(|k| matches!(k, NoteKey::Page { .. })) >= 3);
    }
}
