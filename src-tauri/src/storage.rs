//! The only transcript persistence in Live Coach. No audio is written anywhere.
//! Every operation is confined to this application's transcript directory.
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct TranscriptStore {
    directory: PathBuf,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn store() -> Result<TranscriptStore, String> {
    // Never fall back to cwd: a platform path failure must not write into a checkout.
    let data = dirs::data_dir().ok_or("Cannot locate this Mac's application support directory.")?;
    Ok(TranscriptStore {
        directory: data.join("com.dnalevity.livecoach/transcripts"),
    })
}

impl TranscriptStore {
    fn check_directory(path: &Path) -> Result<bool, String> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(
                "Transcript storage path must be an ordinary directory, not a link or file.".into(),
            ),
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }

    fn exists(&self) -> Result<bool, String> {
        if let Some(parent) = self.directory.parent() {
            if !Self::check_directory(parent)? {
                return Ok(false);
            }
        }
        Self::check_directory(&self.directory)
    }

    fn prepare(&self) -> Result<(), String> {
        let parent = self
            .directory
            .parent()
            .ok_or("Invalid transcript storage directory.")?;
        if !Self::check_directory(parent)? {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        if !Self::check_directory(&self.directory)? {
            fs::create_dir(&self.directory).map_err(|e| e.to_string())?;
        }
        fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())
    }

    fn files(&self) -> Result<Vec<PathBuf>, String> {
        if !self.exists()? {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            // Never read linked files or descend into child directories.
            if entry.file_type().map_err(|e| e.to_string())?.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "json")
            {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }

    fn expire_at(&self, current_time: u64) -> Result<(), String> {
        for path in self.files()? {
            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            let expired = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|value| value["expires_at"].as_u64())
                .is_none_or(|expiry| expiry <= current_time);
            // Malformed records have no defensible retention deadline: discard them.
            if expired {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn save_at<T: Serialize>(
        &self,
        segments: &T,
        enabled: bool,
        days: u32,
        current_time: u64,
    ) -> Result<(), String> {
        if !enabled {
            return Ok(());
        }
        self.expire_at(current_time)?;
        self.prepare()?;
        let value = json!({
            "expires_at": current_time.saturating_add(days.clamp(1, 365) as u64 * 86400),
            "segments": segments,
        });
        let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        let path = self
            .directory
            .join(format!("{}.json", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(path);
        }
        result
    }

    fn load_at(&self, current_time: u64) -> Result<Vec<Value>, String> {
        self.expire_at(current_time)?;
        self.files()?
            .into_iter()
            .map(|path| {
                let bytes = fs::read(path).map_err(|e| e.to_string())?;
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())
            })
            .collect()
    }

    fn purge(&self) -> Result<(), String> {
        if self.exists()? {
            // remove_dir_all does not follow symlinks inside the directory.
            fs::remove_dir_all(&self.directory).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub fn purge() -> Result<(), String> {
    store()?.purge()
}
pub fn expire() -> Result<(), String> {
    store()?.expire_at(now())
}
pub fn save_if_enabled<T: Serialize>(segments: &T, enabled: bool, days: u32) -> Result<(), String> {
    // A disabled setting does not even inspect or create the persistence directory.
    if !enabled {
        return Ok(());
    }
    store()?.save_at(segments, true, days, now())
}
pub fn save<T: Serialize>(segments: &T, days: u32) -> Result<(), String> {
    save_if_enabled(segments, true, days)
}
pub fn load() -> Result<Vec<Value>, String> {
    store()?.load_at(now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    struct Fixture {
        root: PathBuf,
        store: TranscriptStore,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("live-coach-storage-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            Self {
                store: TranscriptStore {
                    directory: root.join("app/transcripts"),
                },
                root,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn disabled_retention_never_creates_or_writes_files() {
        let fixture = Fixture::new();
        fixture
            .store
            .save_at(&vec!["sensitive"], false, 7, 100)
            .unwrap();
        assert!(!fixture.store.directory.exists());
        assert!(fixture.store.load_at(100).unwrap().is_empty());
        fixture
            .store
            .save_at(&vec!["opted in"], true, 7, 100)
            .unwrap();
        fixture
            .store
            .save_at(&vec!["must not persist"], false, 7, 100)
            .unwrap();
        let loaded = fixture.store.load_at(100).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0]["segments"][0], "opted in");
    }

    #[test]
    fn expiry_is_enforced_at_deadline_and_malformed_records_are_discarded() {
        let fixture = Fixture::new();
        fixture.store.save_at(&vec!["short"], true, 1, 100).unwrap();
        fixture.store.save_at(&vec!["long"], true, 2, 100).unwrap();
        fs::write(fixture.store.directory.join("bad.json"), "malformed").unwrap();
        fs::write(
            fixture.store.directory.join("missing.json"),
            "{\"segments\":[]}",
        )
        .unwrap();
        assert_eq!(fixture.store.load_at(86499).unwrap().len(), 2);
        let loaded = fixture.store.load_at(86500).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0]["segments"][0], "long");
        assert!(fixture.store.load_at(172900).unwrap().is_empty());
    }

    #[test]
    fn retention_is_clamped_and_permissions_are_private() {
        let fixture = Fixture::new();
        fixture
            .store
            .save_at(&vec!["minimum"], true, 0, 100)
            .unwrap();
        fixture
            .store
            .save_at(&vec!["maximum"], true, 9999, 100)
            .unwrap();
        let loaded = fixture.store.load_at(100).unwrap();
        let mut deadlines = loaded
            .iter()
            .map(|v| v["expires_at"].as_u64().unwrap())
            .collect::<Vec<_>>();
        deadlines.sort();
        assert_eq!(deadlines, vec![86500, 31536100]);
        for directory in [&fixture.store.directory, &fixture.root.join("app")] {
            assert_eq!(
                fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        for path in fixture.store.files().unwrap() {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn instant_deletion_is_confined_to_transcripts_and_does_not_follow_links() {
        let fixture = Fixture::new();
        let outside = fixture.root.join("Meetily-sessions.json");
        fs::write(
            &outside,
            "{\"expires_at\":99999999,\"segments\":[\"original\"]}",
        )
        .unwrap();
        fixture.store.save_at(&vec!["ours"], true, 7, 100).unwrap();
        let sibling = fixture.root.join("app/settings.json");
        fs::write(&sibling, "preserve").unwrap();
        symlink(&outside, fixture.store.directory.join("linked.json")).unwrap();
        assert_eq!(fixture.store.load_at(100).unwrap().len(), 1);
        fixture.store.purge().unwrap();
        assert!(!fixture.store.directory.exists());
        assert!(fs::read_to_string(&outside).unwrap().contains("original"));
        assert_eq!(fs::read_to_string(sibling).unwrap(), "preserve");
        fixture.store.purge().unwrap();
    }

    #[test]
    fn directory_links_cannot_redirect_retention_or_deletion() {
        let fixture = Fixture::new();
        let outside = fixture.root.join("Meetily");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("original.json"), "original").unwrap();
        fs::create_dir(fixture.root.join("app")).unwrap();
        symlink(&outside, &fixture.store.directory).unwrap();
        assert!(fixture
            .store
            .save_at(&vec!["secret"], true, 1, 100)
            .is_err());
        assert!(fixture.store.load_at(100).is_err());
        assert!(fixture.store.purge().is_err());
        assert_eq!(
            fs::read_to_string(outside.join("original.json")).unwrap(),
            "original"
        );
    }
}
