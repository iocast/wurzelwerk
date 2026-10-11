//! Markdown notes live on the file system next to the database:
//!
//! ```text
//! <project>/person/<person-id>/notes.md       (attachments: person/<id>/files/)
//! <project>/notes/clans/<clan-id>.md
//! <project>/notes/books/<book>/<page>.md      global documentation books
//! ```
//!
//! Person and clan notes are keyed by id, so renaming never breaks the link.
//! Books are keyed by their (human readable) folder and file names.
//!
//! A person note starts with a YAML front matter preamble holding the person
//! data (`id`, `name`, `born`, `died`). The database stays the source of truth:
//! the preamble is regenerated on every save and never shown in the editor.

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Id, Person};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteKey {
    Person(Id),
    Clan(Id),
    Page { book: String, page: String },
}

impl NoteKey {
    pub fn path(&self, project: &Path) -> PathBuf {
        let root = project.join("notes");
        match self {
            Self::Person(id) => project.join("person").join(id).join("notes.md"),
            Self::Clan(id) => root.join("clans").join(format!("{id}.md")),
            Self::Page { book, page } => root.join("books").join(book).join(format!("{page}.md")),
        }
    }
}

/// A global documentation book: a folder of markdown pages, sorted by name
/// (prefix pages with `01 `, `02 `, … to order them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    pub name: String,
    pub pages: Vec<String>,
}

fn books_dir(project: &Path) -> PathBuf {
    project.join("notes").join("books")
}

/// Missing file = empty note. Person notes come back without their preamble.
pub fn load(project: &Path, key: &NoteKey) -> String {
    let text = fs::read_to_string(key.path(project)).unwrap_or_default();
    match key {
        NoteKey::Person(_) => split_preamble(&text).to_owned(),
        _ => text,
    }
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// The note file content of a person: front matter + markdown body.
pub fn with_preamble(person: &Person, body: &str) -> String {
    format!(
        "---\nid: {}\nname: {}\nborn: {}\ndied: {}\n---\n\n{}",
        quote(&person.id),
        quote(&person.name),
        quote(&person.born),
        quote(&person.died),
        body
    )
}

/// The markdown after a leading `---` front matter block (or the whole text).
pub fn split_preamble(text: &str) -> &str {
    let text = text.trim_start_matches('\u{feff}');
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return text;
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        offset += line.len();
        if line.trim_end() == "---" {
            return rest[offset..].trim_start_matches(['\r', '\n']);
        }
    }
    text
}

/// Rewrites the preamble of an existing person note after the person changed.
pub fn refresh_preamble(project: &Path, person: &Person) {
    let path = NoteKey::Person(person.id.clone()).path(project);
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    let updated = with_preamble(person, split_preamble(&text));
    if updated != text {
        let _ = save(&path, &updated);
    }
}

/// Moves notes from the old `notes/people/<id>.md` layout to `person/<id>/notes.md`.
pub fn migrate_legacy(project: &Path) {
    let legacy = project.join("notes").join("people");
    let Ok(entries) = fs::read_dir(&legacy) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "md") {
            continue;
        }
        let Some(id) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        let dest = NoteKey::Person(id).path(project);
        if dest.exists() {
            continue;
        }
        if let Some(dir) = dest.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::rename(&p, &dest);
    }
    let _ = fs::remove_dir(&legacy);
}

/// Ids of people with a non-empty note body.
pub fn noted_people(project: &Path) -> Vec<Id> {
    fs::read_dir(project.join("person"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let text = fs::read_to_string(e.path().join("notes.md")).ok()?;
            (!split_preamble(&text).trim().is_empty())
                .then(|| e.file_name().to_string_lossy().into_owned())
        })
        .collect()
}

/// Writes through a temp file + rename so a crash can't truncate a note.
pub fn save(path: &Path, markdown: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, markdown).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("cannot save {}: {e}", path.display()))
}

/// Removes a note; for the last page of a book the empty book folder goes too.
pub fn delete(project: &Path, key: &NoteKey) {
    let path = key.path(project);
    let _ = fs::remove_file(&path);
    if let Some(dir) = path.parent()
        && !matches!(key, NoteKey::Clan(_))
    {
        let _ = fs::remove_dir(dir); // fails (and is ignored) while pages remain
    }
}

pub fn scan_books(project: &Path) -> Vec<Book> {
    let Ok(entries) = fs::read_dir(books_dir(project)) else {
        return Vec::new();
    };
    let mut books: Vec<Book> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let mut pages: Vec<String> = fs::read_dir(e.path())
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|f| f.path())
                .filter(|p| p.extension().is_some_and(|x| x == "md"))
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                .collect();
            pages.sort_by_key(|p| p.to_lowercase());
            Book {
                name: e.file_name().to_string_lossy().into_owned(),
                pages,
            }
        })
        .collect();
    books.sort_by_key(|b| b.name.to_lowercase());
    books
}

/// Creates `Book N/Page 1.md`; returns the key of the new page.
pub fn create_book(project: &Path) -> Result<NoteKey, String> {
    let dir = books_dir(project);
    let book = unique(&dir, "Book", "");
    create_page(project, &book)
}

/// Creates `<book>/Page N.md`; returns its key.
pub fn create_page(project: &Path, book: &str) -> Result<NoteKey, String> {
    let dir = books_dir(project).join(book);
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let page = unique(&dir, "Page", "md");
    let key = NoteKey::Page {
        book: book.to_owned(),
        page,
    };
    save(&key.path(project), "")?;
    Ok(key)
}

/// Renames a page file; returns the new key (the name is made file-system safe).
pub fn rename_page(project: &Path, key: &NoteKey, new_name: &str) -> Result<NoteKey, String> {
    let NoteKey::Page { book, page } = key else {
        return Err("only book pages can be renamed".into());
    };
    let Some(name) = sanitize(new_name) else {
        return Err("the page name is empty".into());
    };
    if &name == page {
        return Ok(key.clone());
    }
    let new_key = NoteKey::Page {
        book: book.clone(),
        page: name,
    };
    let dest = new_key.path(project);
    if dest.exists() {
        return Err("a page with this name already exists".into());
    }
    fs::rename(key.path(project), &dest).map_err(|e| format!("cannot rename: {e}"))?;
    Ok(new_key)
}

/// Strips characters that are invalid in Windows file names.
fn sanitize(name: &str) -> Option<String> {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.').trim().to_owned();
    (!clean.is_empty()).then_some(clean)
}

/// `Base 1`, `Base 2`, … - the first name that is free in `dir`.
fn unique(dir: &Path, base: &str, ext: &str) -> String {
    (1..)
        .map(|n| format!("{base} {n}"))
        .find(|name| {
            let p = if ext.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{name}.{ext}"))
            };
            !p.exists()
        })
        .expect("unbounded range")
}

/// Renames a book folder; returns the (sanitized) new name.
pub fn rename_book(project: &Path, old: &str, new_name: &str) -> Result<String, String> {
    let Some(name) = sanitize(new_name) else {
        return Err("the book name is empty".into());
    };
    if name == old {
        return Ok(name);
    }
    let dest = books_dir(project).join(&name);
    if dest.exists() {
        return Err("a book with this name already exists".into());
    }
    fs::rename(books_dir(project).join(old), &dest).map_err(|e| format!("cannot rename: {e}"))?;
    Ok(name)
}

/// Ids (file stems) of the non-empty person / clan notes in `notes/<kind>/`.
pub fn noted_ids(project: &Path, kind: &str) -> Vec<Id> {
    fs::read_dir(project.join("notes").join(kind))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.metadata().is_ok_and(|m| m.len() > 0))
        .filter_map(|e| {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "md") {
                p.file_stem().map(|s| s.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person() -> Person {
        Person {
            id: "abc".into(),
            name: "Anna \"Ann\": Weber".into(),
            born: "1950-03-01".into(),
            died: String::new(),
            x: 0.0,
            y: 0.0,
            clans: Vec::new(),
        }
    }

    #[test]
    fn preamble_round_trip() {
        let text = with_preamble(&person(), "# Title\n\n---\n\nbody\n");
        assert!(text.starts_with("---\nid: \"abc\"\nname: \"Anna \\\"Ann\\\": Weber\"\n"));
        assert_eq!(split_preamble(&text), "# Title\n\n---\n\nbody\n");
    }

    #[test]
    fn plain_text_is_untouched() {
        assert_eq!(split_preamble("# Hi\n"), "# Hi\n");
        assert_eq!(split_preamble("---\nno closing line"), "---\nno closing line");
    }
}
