//! Attachments live on the file system next to the database:
//! `<project>/person/<person-id>/files/<file name>`
//! (the person's markdown note is `<project>/person/<person-id>/notes.md`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::{FileRec, new_id};

pub fn person_dir(project: &Path, person: &str) -> PathBuf {
    project.join("person").join(person).join("files")
}

/// Moves attachments from the old `files/<person-id>/` layout to
/// `person/<person-id>/files/`. Returns the records whose path changed.
pub fn migrate_legacy(project: &Path, recs: &mut [FileRec]) -> Vec<FileRec> {
    let legacy = project.join("files");
    let Ok(entries) = fs::read_dir(&legacy) else {
        return Vec::new();
    };
    for e in entries.filter_map(Result::ok) {
        let dest = person_dir(project, &e.file_name().to_string_lossy());
        if dest.exists() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::rename(e.path(), &dest);
    }
    let _ = fs::remove_dir(&legacy);

    let mut changed = Vec::new();
    for rec in recs.iter_mut() {
        let prefix = format!("files/{}/", rec.person);
        if let Some(rest) = rec.rel_path.strip_prefix(&prefix) {
            let new = format!("person/{}/files/{rest}", rec.person);
            if project.join(&new).exists() {
                rec.rel_path = new;
                changed.push(rec.clone());
            }
        }
    }
    changed
}

/// Copies files into the project folder. Runs on the async executor, not the UI thread.
pub fn import(project: &Path, person: &str, sources: &[PathBuf]) -> Result<Vec<FileRec>, String> {
    let dir = person_dir(project, person);
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let mut out = Vec::new();
    for src in sources.iter().filter(|p| p.is_file()) {
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());

        let dest = unique_path(&dir, &name);
        fs::copy(src, &dest).map_err(|e| format!("cannot copy {}: {e}", src.display()))?;

        let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        let rel = dest
            .strip_prefix(project)
            .unwrap_or(&dest)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        out.push(FileRec {
            id: new_id(),
            person: person.to_owned(),
            name: dest
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or(name),
            rel_path: rel,
            size,
        });
    }
    Ok(out)
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_owned());
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    (1..)
        .map(|n| match &ext {
            Some(ext) => dir.join(format!("{stem} ({n}).{ext}")),
            None => dir.join(format!("{stem} ({n})")),
        })
        .find(|p| !p.exists())
        .unwrap_or(candidate)
}

pub fn absolute(project: &Path, rec: &FileRec) -> PathBuf {
    project.join(&rec.rel_path)
}

/// Opens a file with the OS default application.
pub fn open(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = {
        use std::os::windows::process::CommandExt;
        // the app has no console; without this flag `cmd` flashes one up
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    };
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(path).spawn();

    result
        .map(|_| ())
        .map_err(|e| format!("cannot open {}: {e}", path.display()))
}

pub fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / KB / KB)
    } else {
        format!("{:.1} GB", b / KB / KB / KB)
    }
}
