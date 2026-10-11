//! Synthetic data for load testing: `--demo 40x250` = 40 clans × 250 people.

use std::collections::VecDeque;

use crate::model::{Clan, Id, Link, PALETTE, Person, new_id};

const SURNAMES: [&str; 24] = [
    "Weber", "Roth", "Keller", "Meier", "Huber", "Frei", "Brunner", "Baumann", "Gerber", "Widmer",
    "Steiner", "Fischer", "Moser", "Wyss", "Graf", "Bühler", "Zürcher", "Hofer", "Suter", "Kunz",
    "Lehmann", "Marti", "Vogel", "Ammann",
];
const FIRST: [&str; 24] = [
    "Anna",
    "Jonas",
    "Karl",
    "Ines",
    "Heinrich",
    "Margarete",
    "Walter",
    "Lea",
    "Noah",
    "Mia",
    "Luca",
    "Emma",
    "Elias",
    "Lina",
    "David",
    "Sofia",
    "Leon",
    "Laura",
    "Nico",
    "Sara",
    "Marc",
    "Julia",
    "Felix",
    "Nina",
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
}

fn clan_name(i: usize) -> String {
    let base = SURNAMES[i % SURNAMES.len()];
    match i / SURNAMES.len() {
        0 => base.to_string(),
        n => format!("{base} {}", n + 1),
    }
}

fn person(rng: &mut Rng, surname: &str, generation: i32, clans: Vec<Id>) -> Person {
    let born = 1880 + generation * 27 + rng.below(8) as i32;
    let died = if born < 1950 {
        (born + 60 + rng.below(30) as i32).to_string()
    } else {
        String::new()
    };
    Person {
        id: new_id(),
        name: format!(
            "{} {surname}",
            FIRST[rng.below(FIRST.len() as u64) as usize]
        ),
        born: born.to_string(),
        died,
        x: 0.0,
        y: 0.0,
        clans,
    }
}

fn link(people: &[Person], a: usize, b: usize, label: &str) -> Link {
    Link {
        id: new_id(),
        from: people[a].id.clone(),
        to: people[b].id.clone(),
        label: label.to_string(),
        route: Vec::new(),
        start_port: None,
        end_port: None,
        label_position: None,
    }
}

pub fn generate(clan_count: usize, per_clan: usize) -> (Vec<Clan>, Vec<Person>, Vec<Link>) {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let clans: Vec<Clan> = (0..clan_count)
        .map(|i| Clan {
            id: new_id(),
            name: clan_name(i),
            color: PALETTE[i % PALETTE.len()],
            label_offset: [0.0, 0.0],
        })
        .collect();

    let mut people: Vec<Person> = Vec::with_capacity(clan_count * per_clan);
    let mut links: Vec<Link> = Vec::with_capacity(clan_count * per_clan * 2);

    for (ci, clan) in clans.iter().enumerate() {
        let start = people.len();
        let full = |people: &Vec<Person>| people.len() - start >= per_clan;
        let other = |rng: &mut Rng| SURNAMES[rng.below(SURNAMES.len() as u64) as usize];

        let a = people.len();
        people.push(person(&mut rng, &clan.name, 0, vec![clan.id.clone()]));
        if full(&people) {
            continue;
        }
        let spouse_name = other(&mut rng);
        let b = people.len();
        people.push(person(&mut rng, spouse_name, 0, vec![clan.id.clone()]));
        links.push(link(&people, a, b, "married"));

        let mut couples = VecDeque::from([(a, b, 0i32)]);
        while !full(&people) {
            let Some((pa, pb, generation)) = couples.pop_front() else {
                break;
            };
            let mut kids = Vec::new();
            for _ in 0..(2 + rng.below(3)) {
                if full(&people) {
                    break;
                }
                let k = people.len();
                people.push(person(
                    &mut rng,
                    &clan.name,
                    generation + 1,
                    vec![clan.id.clone()],
                ));
                links.push(link(&people, pa, k, "parents of"));
                links.push(link(&people, pb, k, "parents of"));
                kids.push(k);
            }
            if kids.len() >= 2 {
                links.push(link(&people, kids[0], kids[1], "siblings"));
            }
            if let Some(&first) = kids.first()
                && rng.below(25) == 0
            {
                links.push(link(&people, pa, first, "godfather of"));
            }
            for &k in &kids {
                if full(&people) {
                    break;
                }
                if rng.below(5) == 0 {
                    continue;
                }
                let mut spouse_clans = vec![clan.id.clone()];
                if clan_count > 1 && rng.below(8) == 0 {
                    spouse_clans.push(clans[(ci + 1) % clan_count].id.clone());
                }
                let spouse_name = other(&mut rng);
                let s = people.len();
                people.push(person(&mut rng, spouse_name, generation + 1, spouse_clans));
                links.push(link(&people, k, s, "married"));
                couples.push_back((k, s, generation + 1));
            }
        }
    }

    (clans, people, links)
}
