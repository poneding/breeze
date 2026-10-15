use crate::epub::{Book, Frame, Request};
use anyhow::{Result, anyhow};
use futures::channel::oneshot;
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
};

struct Job {
    request: Request,
    reply: oneshot::Sender<Result<Frame>>,
}
#[derive(Default)]
struct Pending {
    newest: Option<Job>,
    stop: bool,
}

/// One running render and one replaceable pending render, independent of the PDF queue.
pub struct EpubWorker {
    queue: Arc<(Mutex<Pending>, Condvar)>,
}

impl EpubWorker {
    pub fn new() -> Result<Self> {
        let queue = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let worker = queue.clone();
        thread::Builder::new()
            .name("breeze-epub".into())
            .spawn(move || {
                let mut loaded: Option<(PathBuf, Book)> = None;
                let mut revision = None;
                let mut cached: Option<(Request, Frame)> = None;
                loop {
                    let (lock, wake) = &*worker;
                    let Ok(mut pending) = lock.lock() else { return };
                    while pending.newest.is_none() && !pending.stop {
                        pending = match wake.wait(pending) {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                    }
                    if pending.stop {
                        return;
                    }
                    let Some(job) = pending.newest.take() else {
                        continue;
                    };
                    drop(pending);
                    let result = (|| {
                        let info = std::fs::metadata(&job.request.path)?;
                        let current_revision = (info.len(), info.modified()?);
                        if revision != Some(current_revision) {
                            loaded = None;
                            cached = None;
                            revision = Some(current_revision);
                        }
                        if let Some((key, frame)) = &cached
                            && *key == job.request
                        {
                            return Ok(frame.clone());
                        }
                        if loaded
                            .as_ref()
                            .is_none_or(|(path, _)| path != &job.request.path)
                        {
                            loaded = None;
                            cached = None;
                            loaded =
                                Some((job.request.path.clone(), Book::open(&job.request.path)?));
                        }
                        let book = &mut loaded
                            .as_mut()
                            .ok_or_else(|| anyhow!("EPUB worker lost its document"))?
                            .1;
                        let frame = book.render(&job.request)?;
                        cached = Some((job.request.clone(), frame.clone()));
                        Ok(frame)
                    })();
                    let _ = job.reply.send(result);
                }
            })?;
        Ok(Self { queue })
    }

    pub fn request(&self, request: Request) -> Result<oneshot::Receiver<Result<Frame>>> {
        let (reply, receive) = oneshot::channel();
        let (lock, wake) = &*self.queue;
        let mut pending = lock
            .lock()
            .map_err(|_| anyhow!("EPUB worker is unavailable"))?;
        if let Some(previous) = pending.newest.replace(Job { request, reply }) {
            let _ = previous
                .reply
                .send(Err(anyhow!("Superseded by a newer EPUB request")));
        }
        wake.notify_one();
        Ok(receive)
    }
}

impl Drop for EpubWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        if let Ok(mut pending) = lock.lock() {
            pending.stop = true;
            pending.newest = None;
            wake.notify_one();
        }
    }
}
