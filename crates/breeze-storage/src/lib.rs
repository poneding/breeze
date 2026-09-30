//! Local, atomic persistence. All public I/O functions must run outside the UI thread.
use anyhow::{Context, Result, bail, ensure};
use breeze_core::annotations::Annotation;
use futures::channel::oneshot;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};

const STATE_LIMIT: u64 = 4 * 1024 * 1024;
const SIDECAR_LIMIT: u64 = 16 * 1024 * 1024;
pub fn data_directory() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("BREEZE_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    Ok(dirs::data_local_dir()
        .context("No OS application-data directory")?
        .join("Breeze"))
}
pub fn fingerprint(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
pub fn bytes_fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "File exceeds size limit: {}",
        path.display()
    );
    Ok(bytes)
}
pub fn atomic_write(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    let parent = path.parent().context("Missing destination directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    if replace {
        file.persist(path).map_err(|e| e.error)?;
    } else {
        file.persist_noclobber(path).map_err(|e| e.error)?;
    }
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}
struct WriteJob<T> {
    value: T,
    reply: oneshot::Sender<Result<()>>,
}
struct Queue<T> {
    pending: Option<WriteJob<T>>,
    stop: bool,
}
type SharedQueue<T> = Arc<(Mutex<Queue<T>>, Condvar)>;
pub struct StateStore<T> {
    queue: SharedQueue<T>,
}
impl<T: Serialize + DeserializeOwned + Default + Send + 'static> StateStore<T> {
    pub fn start(root: PathBuf) -> Result<(Self, oneshot::Receiver<Result<T>>)> {
        let queue = Arc::new((
            Mutex::new(Queue {
                pending: None::<WriteJob<T>>,
                stop: false,
            }),
            Condvar::new(),
        ));
        let q = queue.clone();
        let (ready, rx) = oneshot::channel();
        thread::Builder::new()
            .name("breeze-state".into())
            .spawn(move || {
                let initial = (|| -> Result<(File, T, Option<String>)> {
                    fs::create_dir_all(&root)?;
                    let lock = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(root.join("state.lock"))?;
                    lock.try_lock()
                        .context("Another Breeze process is using this data directory")?;
                    let path = root.join("state.json");
                    let (state, revision) = if path.try_exists()? {
                        let bytes = read_bounded(&path, STATE_LIMIT)?;
                        (
                            serde_json::from_slice(&bytes)
                                .context("State file is invalid; it will not be overwritten")?,
                            Some(bytes_fingerprint(&bytes)),
                        )
                    } else {
                        (T::default(), None)
                    };
                    Ok((lock, state, revision))
                })();
                let (lock, state, mut revision) = match initial {
                    Ok(v) => v,
                    Err(e) => {
                        if let Ok(mut pending) = q.0.lock() {
                            pending.stop = true;
                        }
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                let _guard = lock;
                let _ = ready.send(Ok(state));
                loop {
                    let Ok(mut pending) = q.0.lock() else {
                        return;
                    };
                    while pending.pending.is_none() && !pending.stop {
                        pending = match q.1.wait(pending) {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                    }
                    let job = pending.pending.take();
                    let stop = pending.stop;
                    drop(pending);
                    if let Some(job) = job {
                        let result = (|| -> Result<()> {
                            let bytes = serde_json::to_vec_pretty(&job.value)?;
                            ensure!(
                                bytes.len() as u64 <= STATE_LIMIT,
                                "Application state exceeds size limit"
                            );
                            let path = root.join("state.json");
                            let current = if path.try_exists()? {
                                Some(bytes_fingerprint(&read_bounded(&path, STATE_LIMIT)?))
                            } else {
                                None
                            };
                            ensure!(
                                current == revision,
                                "Application state changed externally; refusing overwrite"
                            );
                            atomic_write(&path, &bytes, revision.is_some())?;
                            revision = Some(bytes_fingerprint(&bytes));
                            Ok(())
                        })();
                        let _ = job.reply.send(result);
                    }
                    if stop {
                        return;
                    }
                }
            })?;
        Ok((Self { queue }, rx))
    }
    pub fn save(&self, value: T) -> Result<oneshot::Receiver<Result<()>>> {
        let (tx, rx) = oneshot::channel();
        let mut q = self
            .queue
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("State writer unavailable"))?;
        ensure!(!q.stop, "State writer is stopped");
        q.pending = Some(WriteJob { value, reply: tx });
        self.queue.1.notify_one();
        Ok(rx)
    }
}
impl<T> Drop for StateStore<T> {
    fn drop(&mut self) {
        if let Ok(mut q) = self.queue.0.lock() {
            q.stop = true;
            self.queue.1.notify_one();
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sidecar {
    version: u32,
    fingerprint: String,
    annotations: Vec<Annotation>,
}
#[derive(Clone, Debug, Default)]
pub struct AnnotationFile {
    pub annotations: Vec<Annotation>,
    pub revision: Option<String>,
}
pub fn sidecar_path(source: &Path) -> PathBuf {
    let mut name = source.as_os_str().to_os_string();
    name.push(".breeze.json");
    PathBuf::from(name)
}
pub fn load_annotations(path: &Path, source_fingerprint: &str) -> Result<AnnotationFile> {
    if !path.try_exists()? {
        return Ok(AnnotationFile::default());
    }
    let bytes = read_bounded(path, SIDECAR_LIMIT)?;
    let data: Sidecar = serde_json::from_slice(&bytes)
        .context("Invalid annotation sidecar; not loaded or overwritten")?;
    ensure!(data.version == 1, "Unsupported annotation version");
    ensure!(
        data.fingerprint == source_fingerprint,
        "PDF has changed; annotations must be reassociated explicitly"
    );
    validate_annotations(&data.annotations)?;
    Ok(AnnotationFile {
        annotations: data.annotations,
        revision: Some(bytes_fingerprint(&bytes)),
    })
}
fn validate_annotations(items: &[Annotation]) -> Result<()> {
    ensure!(items.len() <= 5000, "Too many annotations");
    let points: usize = items.iter().map(|a| a.points.len() + a.quads.len()).sum();
    ensure!(points <= 100_000, "Annotation geometry exceeds budget");
    for a in items {
        a.validate().map_err(anyhow::Error::msg)?;
    }
    Ok(())
}
pub fn save_annotations(
    path: &Path,
    source: Option<&Path>,
    expected_fingerprint: &str,
    expected_revision: Option<&str>,
    annotations: Vec<Annotation>,
) -> Result<String> {
    validate_annotations(&annotations)?;
    let lock_path = path.with_extension("json.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    lock.try_lock()
        .context("Another annotation writer is active")?;
    if let Some(source) = source {
        ensure!(
            fingerprint(source)? == expected_fingerprint,
            "Source changed; annotation save refused"
        );
    }
    match (path.try_exists()?, expected_revision) {
        (true, Some(revision)) => ensure!(
            bytes_fingerprint(&read_bounded(path, SIDECAR_LIMIT)?) == revision,
            "Sidecar changed externally; save refused"
        ),
        (false, None) => {}
        _ => bail!("Sidecar existence changed; save refused"),
    }
    let bytes = serde_json::to_vec_pretty(&Sidecar {
        version: 1,
        fingerprint: expected_fingerprint.into(),
        annotations,
    })?;
    ensure!(
        bytes.len() as u64 <= SIDECAR_LIMIT,
        "Sidecar exceeds size limit"
    );
    atomic_write(path, &bytes, expected_revision.is_some())?;
    Ok(bytes_fingerprint(&bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_roundtrip_and_lock() {
        let root = tempfile::tempdir().unwrap();
        let (store, ready) = StateStore::<Vec<String>>::start(root.path().into()).unwrap();
        assert!(
            futures::executor::block_on(ready)
                .unwrap()
                .unwrap()
                .is_empty()
        );
        let (_second, ready2) = StateStore::<Vec<String>>::start(root.path().into()).unwrap();
        assert!(futures::executor::block_on(ready2).unwrap().is_err());
        futures::executor::block_on(store.save(vec!["saved".into()]).unwrap())
            .unwrap()
            .unwrap();
        let bytes = fs::read(root.path().join("state.json")).unwrap();
        assert_eq!(
            serde_json::from_slice::<Vec<String>>(&bytes).unwrap(),
            vec!["saved"]
        );
    }
    #[test]
    fn corrupt_state_is_not_reset() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.json");
        fs::write(&path, "broken").unwrap();
        let (_store, ready) = StateStore::<Vec<String>>::start(root.path().into()).unwrap();
        assert!(futures::executor::block_on(ready).unwrap().is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "broken");
    }
    #[test]
    fn sidecar_conflicts_preserve_files() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("test.pdf");
        fs::write(&pdf, b"source").unwrap();
        let hash = fingerprint(&pdf).unwrap();
        let path = sidecar_path(&pdf);
        let rev = save_annotations(&path, Some(&pdf), &hash, None, vec![]).unwrap();
        assert!(load_annotations(&path, &hash).is_ok());
        assert!(save_annotations(&path, Some(&pdf), &hash, None, vec![]).is_err());
        fs::write(&pdf, b"changed").unwrap();
        assert!(save_annotations(&path, Some(&pdf), &hash, Some(&rev), vec![]).is_err());
        assert!(load_annotations(&path, "wrong").is_err());
    }
    #[test]
    fn atomic_creation_never_clobbers() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("a");
        atomic_write(&path, b"old", false).unwrap();
        assert!(atomic_write(&path, b"new", false).is_err());
        assert_eq!(fs::read(path).unwrap(), b"old");
    }
}

#[cfg(test)]
mod annotation_tests {
    use super::*;
    use breeze_core::annotations::{Kind, Point};
    #[test]
    fn annotation_roundtrip_and_external_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("source.pdf");
        fs::write(&pdf, b"immutable").unwrap();
        let hash = fingerprint(&pdf).unwrap();
        let path = sidecar_path(&pdf);
        let annotation = Annotation {
            page: 0,
            kind: Kind::Ink,
            points: vec![Point::new(10., 20.), Point::new(30., 40.)],
            quads: vec![],
            color: [0.2, 0.3, 0.4],
            opacity: 0.5,
            width: 2.,
            text: String::new(),
        };
        let revision =
            save_annotations(&path, Some(&pdf), &hash, None, vec![annotation.clone()]).unwrap();
        let loaded = load_annotations(&path, &hash).unwrap();
        assert_eq!(loaded.annotations, vec![annotation]);
        fs::write(&path, b"external edit").unwrap();
        assert!(save_annotations(&path, Some(&pdf), &hash, Some(&revision), vec![]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external edit");
        assert_eq!(fs::read(&pdf).unwrap(), b"immutable");
    }
    #[test]
    fn failed_initialization_rejects_future_saves() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("state.json"), b"bad").unwrap();
        let (store, rx) = StateStore::<Vec<String>>::start(dir.path().into()).unwrap();
        assert!(futures::executor::block_on(rx).unwrap().is_err());
        assert!(store.save(vec![]).is_err());
    }
}

#[cfg(test)]
mod state_conflict_tests {
    use super::*;
    #[test]
    fn external_state_edits_are_not_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let (store, ready) = StateStore::<Vec<String>>::start(root.path().into()).unwrap();
        futures::executor::block_on(ready).unwrap().unwrap();
        futures::executor::block_on(store.save(vec!["initial".into()]).unwrap())
            .unwrap()
            .unwrap();
        let path = root.path().join("state.json");
        fs::write(&path, b"[\"external\"]").unwrap();
        assert!(
            futures::executor::block_on(store.save(vec!["stale".into()]).unwrap())
                .unwrap()
                .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), b"[\"external\"]");
    }
}
