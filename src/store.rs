//! Persistence: embedded RedDB (document collections) on a dedicated worker thread.
//!
//! * DB file: `<project>/wurzelwerk.rdb` (single-file embedded packaging)
//! * One document collection per entity type. Every document has the shape
//!   `{"id": "<uuid>", "data": { ...entity... }}`.
//! * The UI thread never blocks on the database: every write is a [`Cmd`] sent
//!   over a channel; the worker applies it and checkpoints when idle.
//!
//! RedDB API used (docs.rs `reddb-io` / reddb-io GitHub):
//! `RedDBRuntime::with_options(RedDBOptions::persistent(path))`,
//! `RedDBRuntime::execute_query(sql)` -> `RuntimeQueryResult { result: UnifiedResult, .. }`,
//! `RedDBRuntime::checkpoint()`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use reddb::{RedDBOptions, RedDBRuntime};
use reddb_types::Value as ReddbValue;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::model::{Clan, Data, FileRec, Link, Person, Relation};

pub const DB_FILE: &str = "wurzelwerk.rdb";

/// Projects created before the rename to Wurzelwerk have `sketchbook.rdb` (+ sidecar files).
pub fn migrate_legacy_db(project: &std::path::Path) {
    const OLD: &str = "sketchbook.rdb";
    if project.join(DB_FILE).exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(project) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(rest) = name.strip_prefix(OLD) {
            let _ = std::fs::rename(e.path(), project.join(format!("{DB_FILE}{rest}")));
        }
    }
}

pub const PEOPLE: &str = "sb_people";
pub const CLANS: &str = "sb_clans";
pub const LINKS: &str = "sb_links";
pub const FILES: &str = "sb_files";
pub const RELATIONS: &str = "sb_relations";

/// A write operation for the worker.
#[derive(Debug)]
pub enum Cmd {
    /// Insert or replace a document. `fresh = true` skips the delete (new entity).
    Put {
        coll: &'static str,
        id: String,
        data: serde_json::Value,
        fresh: bool,
    },
    Del {
        coll: &'static str,
        id: String,
    },
    Batch(Vec<Cmd>),
    /// Checkpoints and answers once every earlier command is written ([`Db::sync`]).
    Sync(Sender<()>),
}

impl Cmd {
    pub fn put<T: Serialize>(coll: &'static str, id: &str, value: &T, fresh: bool) -> Self {
        Cmd::Put {
            coll,
            id: id.to_owned(),
            data: serde_json::to_value(value).unwrap_or_default(),
            fresh,
        }
    }

    pub fn del(coll: &'static str, id: &str) -> Self {
        Cmd::Del {
            coll,
            id: id.to_owned(),
        }
    }
}

/// Handle owned by the UI. Cheap to clone.
#[derive(Clone)]
pub struct Db {
    tx: Sender<Cmd>,
    pending: Arc<AtomicUsize>,
    error: Arc<Mutex<Option<String>>>,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db")
            .field("pending", &self.pending())
            .finish()
    }
}

impl Db {
    /// Opens (or creates) the database on a worker thread and loads everything.
    /// The RedDB runtime is created *inside* the worker, so it never has to cross threads.
    pub fn open(path: PathBuf) -> Result<(Db, Data), String> {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (init_tx, init_rx) = mpsc::channel::<Result<Data, String>>();
        let pending = Arc::new(AtomicUsize::new(0));
        let error = Arc::new(Mutex::new(None::<String>));

        let worker_pending = pending.clone();
        let worker_error = error.clone();

        thread::Builder::new()
            .name("reddb-worker".into())
            .spawn(move || {
                let store = match Store::open(&path) {
                    Ok(store) => store,
                    Err(e) => {
                        let _ = init_tx.send(Err(e));
                        return;
                    }
                };
                let _ = init_tx.send(store.load_all());

                while let Ok(cmd) = rx.recv() {
                    if let Err(e) = store.apply(cmd) {
                        eprintln!("[reddb] {e}");
                        if let Ok(mut slot) = worker_error.lock() {
                            slot.get_or_insert(e);
                        }
                    }
                    if worker_pending.fetch_sub(1, Ordering::SeqCst) == 1 {
                        // queue drained -> fold the WAL into the main file
                        store.checkpoint();
                    }
                }
                store.checkpoint();
            })
            .map_err(|e| format!("cannot start db worker: {e}"))?;

        let data = init_rx
            .recv()
            .map_err(|_| "db worker stopped during startup".to_string())??;

        Ok((Db { tx, pending, error }, data))
    }

    pub fn send(&self, cmd: Cmd) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        if self.tx.send(cmd).is_err() {
            self.pending.fetch_sub(1, Ordering::SeqCst);
            if let Ok(mut slot) = self.error.lock() {
                slot.get_or_insert_with(|| "db worker is not running".into());
            }
        }
    }

    /// Blocks until everything sent so far is written to the database file.
    pub fn sync(&self) {
        let (done_tx, done_rx) = mpsc::channel();
        self.send(Cmd::Sync(done_tx));
        let _ = done_rx.recv_timeout(std::time::Duration::from_secs(10));
    }

    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }

    pub fn last_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|slot| slot.clone())
    }
}

/// Thin adapter over the embedded RedDB runtime. Lives on the worker thread only.
struct Store {
    rt: RedDBRuntime,
}

impl Store {
    fn open(path: &Path) -> Result<Self, String> {
        let rt = RedDBRuntime::with_options(RedDBOptions::persistent(path))
            .map_err(|e| format!("cannot open {}: {e:?}", path.display()))?;
        for coll in [PEOPLE, CLANS, LINKS, FILES, RELATIONS] {
            let sql = format!("CREATE DOCUMENT IF NOT EXISTS {coll}");
            rt.execute_query(&sql)
                .map_err(|e| format!("{sql}: {e:?}"))?;
        }
        Ok(Self { rt })
    }

    fn load_all(&self) -> Result<Data, String> {
        let people: Vec<Person> = self.load(PEOPLE)?;
        let clans: Vec<Clan> = self.load(CLANS)?;
        let links: Vec<Link> = self.load(LINKS)?;
        let files: Vec<FileRec> = self.load(FILES)?;
        let mut data = Data::from_parts(people, clans, links, files);
        data.relations = self.load::<Relation>(RELATIONS)?;
        Ok(data)
    }

    /// Read the entity payload from each native document's `body.data` path.
    fn load<T: DeserializeOwned>(&self, coll: &str) -> Result<Vec<T>, String> {
        let sql = format!("SELECT body.\"data\" AS payload FROM {coll}");
        let res = match self.rt.execute_query(sql.as_str()) {
            Ok(res) => res,
            Err(e) => {
                let msg = format!("{e:?}");
                if is_not_found(&msg) {
                    return Ok(Vec::new());
                }
                return Err(format!("{sql}: {msg}"));
            }
        };

        let mut out = Vec::with_capacity(res.result.records.len());
        for record in res.result.records.iter() {
            let Some(value) = record.get("payload") else {
                continue;
            };
            let parsed = match value {
                ReddbValue::Json(json) => serde_json::from_slice::<T>(json),
                ReddbValue::Text(json) => serde_json::from_str::<T>(json),
                _ => continue,
            };
            match parsed {
                Ok(item) => out.push(item),
                Err(e) => eprintln!("[reddb] skipping bad document in {coll}: {e}"),
            }
        }
        Ok(out)
    }

    fn apply(&self, cmd: Cmd) -> Result<(), String> {
        match cmd {
            Cmd::Put {
                coll,
                id,
                data,
                fresh,
            } => {
                if !fresh {
                    self.delete(coll, &id)?;
                }
                let body = serde_json::json!({ "id": id, "data": data });
                self.exec(&format!("INSERT INTO {coll} DOCUMENT VALUES ({})", body))
            }
            Cmd::Del { coll, id } => self.delete(coll, &id),
            Cmd::Batch(cmds) => {
                let mut first_err = None;
                for cmd in cmds {
                    if let Err(e) = self.apply(cmd) {
                        first_err.get_or_insert(e);
                    }
                }
                first_err.map_or(Ok(()), Err)
            }
            Cmd::Sync(done) => {
                self.checkpoint();
                let _ = done.send(());
                Ok(())
            }
        }
    }

    fn delete(&self, coll: &str, id: &str) -> Result<(), String> {
        let sql = format!("DELETE FROM {coll} WHERE body.id = {}", sql_string(id));
        match self.rt.execute_query(sql.as_str()) {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = format!("{e:?}");
                if is_not_found(&msg) {
                    Ok(())
                } else {
                    Err(format!("{sql}: {msg}"))
                }
            }
        }
    }

    fn exec(&self, sql: &str) -> Result<(), String> {
        self.rt.execute_query(sql).map(|_| ()).map_err(|e| {
            let short: String = sql.chars().take(120).collect();
            format!("{short}: {e:?}")
        })
    }

    fn checkpoint(&self) {
        if let Err(e) = self.rt.checkpoint() {
            eprintln!("[reddb] checkpoint failed: {e:?}");
        }
    }
}

fn is_not_found(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("not found") || m.contains("notfound") || m.contains("does not exist")
}

/// Standard SQL string literal ('' escapes a quote).
fn sql_string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_collection_round_trips_entities() {
        let rt = RedDBRuntime::with_options(RedDBOptions::in_memory()).unwrap();
        let store = Store { rt };
        store
            .rt
            .execute_query(&format!("CREATE DOCUMENT IF NOT EXISTS {PEOPLE}"))
            .unwrap();

        let person = Person {
            id: "person-1".into(),
            name: "Ada".into(),
            born: "1815".into(),
            died: String::new(),
            x: 12.0,
            y: 34.0,
            clans: Vec::new(),
        };
        store
            .apply(Cmd::put(PEOPLE, &person.id, &person, true))
            .unwrap();
        assert_eq!(store.load::<Person>(PEOPLE).unwrap(), vec![person.clone()]);

        let mut updated = person.clone();
        updated.name = "Ada Lovelace".into();
        store
            .apply(Cmd::put(PEOPLE, &updated.id, &updated, false))
            .unwrap();
        assert_eq!(store.load::<Person>(PEOPLE).unwrap(), vec![updated.clone()]);

        store.apply(Cmd::del(PEOPLE, &updated.id)).unwrap();
        assert!(store.load::<Person>(PEOPLE).unwrap().is_empty());
    }
}
