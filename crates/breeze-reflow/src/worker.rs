use crate::epub::{Book, Frame, Request};
use anyhow::{Result, anyhow};
use futures::channel::oneshot;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
};

/// Pages kept ready inside the worker. A page turn then hands over an existing
/// raster instead of re-laying out and re-rasterizing the chapter, which is what
/// made turning a page feel slow.
const CACHE_PAGES: usize = 4;
/// Upper bound for the cached rasters, so a large viewport cannot grow the
/// worker without limit.
const CACHE_BYTES: usize = 96 * 1024 * 1024;

struct Job {
    request: Request,
    reply: oneshot::Sender<Result<Frame>>,
    /// A speculative page the UI did not ask for. It stays behind visible work.
    prefetch: bool,
}

#[derive(Default)]
struct Pending {
    newest: Option<Job>,
    prefetch: Option<Job>,
    stop: bool,
}

impl Pending {
    fn take_next(&mut self) -> Option<Job> {
        self.newest.take().or_else(|| self.prefetch.take())
    }

    fn is_empty(&self) -> bool {
        self.newest.is_none() && self.prefetch.is_none()
    }
}

struct PageCache {
    entries: VecDeque<(Request, Frame)>,
    bytes: usize,
}

impl PageCache {
    fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
        }
    }

    fn get(&self, request: &Request) -> Option<Frame> {
        self.entries
            .iter()
            .find(|(key, _)| key == request)
            .map(|(_, frame)| frame.clone())
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    fn insert(&mut self, request: Request, frame: Frame) {
        let size = frame
            .pages
            .iter()
            .map(|page| page.bgra.len())
            .sum::<usize>();
        self.entries.push_back((request, frame));
        self.bytes += size;
        while self.entries.len() > CACHE_PAGES
            || (self.bytes > CACHE_BYTES && self.entries.len() > 1)
        {
            if let Some((_, old)) = self.entries.pop_front() {
                let old_size = old.pages.iter().map(|page| page.bgra.len()).sum::<usize>();
                self.bytes = self.bytes.saturating_sub(old_size);
            }
        }
    }
}

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
                let mut cache = PageCache::new();
                loop {
                    let (lock, wake) = &*worker;
                    let Ok(mut pending) = lock.lock() else { return };
                    while pending.is_empty() && !pending.stop {
                        pending = match wake.wait(pending) {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                    }
                    if pending.stop {
                        return;
                    }
                    let Some(job) = pending.take_next() else {
                        continue;
                    };
                    drop(pending);
                    let prefetch = job.prefetch;
                    let result = (|| {
                        let info = std::fs::metadata(&job.request.path)?;
                        let current_revision = (info.len(), info.modified()?);
                        if revision != Some(current_revision) {
                            loaded = None;
                            cache.clear();
                            revision = Some(current_revision);
                        }
                        if let Some(frame) = cache.get(&job.request) {
                            return Ok(frame);
                        }
                        if loaded
                            .as_ref()
                            .is_none_or(|(path, _)| path != &job.request.path)
                        {
                            loaded = None;
                            cache.clear();
                            loaded =
                                Some((job.request.path.clone(), Book::open(&job.request.path)?));
                        }
                        let book = &mut loaded
                            .as_mut()
                            .ok_or_else(|| anyhow!("EPUB worker lost its document"))?
                            .1;
                        let frame = book.render(&job.request)?;
                        cache.insert(job.request.clone(), frame.clone());
                        Ok(frame)
                    })();
                    if prefetch {
                        // A speculative page has no waiter; drop the result and
                        // keep only what the cache learned.
                        continue;
                    }
                    let _ = job.reply.send(result);
                }
            })?;
        Ok(Self { queue })
    }

    pub fn request(&self, request: Request) -> Result<oneshot::Receiver<Result<Frame>>> {
        self.enqueue(request, false)
    }

    /// Prepare a page in the background. It never displaces a visible request
    /// and its result is only cached.
    pub fn prefetch(&self, request: Request) -> Result<()> {
        self.enqueue(request, true).map(|_| ())
    }

    fn enqueue(
        &self,
        request: Request,
        prefetch: bool,
    ) -> Result<oneshot::Receiver<Result<Frame>>> {
        let (reply, receive) = oneshot::channel();
        let (lock, wake) = &*self.queue;
        let mut pending = lock
            .lock()
            .map_err(|_| anyhow!("EPUB worker is unavailable"))?;
        let job = Job {
            request,
            reply,
            prefetch,
        };
        let previous = if prefetch {
            pending.prefetch.replace(job)
        } else {
            pending.newest.replace(job)
        };
        if let Some(previous) = previous
            && !prefetch
        {
            let _ = previous
                .reply
                .send(Err(anyhow!("Superseded by a newer EPUB request")));
        }
        wake.notify_one();
        drop(pending);
        Ok(receive)
    }
}

impl Drop for EpubWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        if let Ok(mut pending) = lock.lock() {
            pending.stop = true;
            pending.newest = None;
            pending.prefetch = None;
            wake.notify_one();
        }
    }
}
