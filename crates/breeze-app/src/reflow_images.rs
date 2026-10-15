//! Local Markdown images are decoded off-thread and retained within a byte budget.
//!
//! A Markdown document may reference remote, missing or unsupported images. A
//! failure is a property of that one image, so it is recorded per URI and
//! rendered as an empty placeholder; it never becomes the reader's status
//! notice, which is reserved for the document as a whole.
use crate::*;
use std::{
    collections::{HashSet, VecDeque},
    io::{Cursor, Read},
    path::Path,
};

const IMAGE_BUDGET: usize = 32 * 1024 * 1024;
const IMAGE_COUNT: usize = 64;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

enum Loaded {
    Image(Arc<RenderImage>),
    Failed,
}

struct Entry {
    uri: String,
    loaded: Loaded,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct Images {
    entries: VecDeque<Entry>,
    loading: HashSet<String>,
    /// Images the document referenced that could not be shown, for the
    /// document information panel.
    pub failures: usize,
    pub last_failure: Option<String>,
}

impl Images {
    pub fn clear(&mut self, cx: &mut App) {
        self.loading.clear();
        self.failures = 0;
        self.last_failure = None;
        for entry in self.entries.drain(..) {
            if let Loaded::Image(image) = entry.loaded {
                cx.defer(move |cx| cx.drop_image(image, None));
            }
        }
    }

    fn insert(&mut self, cx: &mut App, entry: Entry) {
        let bytes = entry.bytes;
        while self.entries.len() >= IMAGE_COUNT
            || self.entries.iter().map(|entry| entry.bytes).sum::<usize>() + bytes > IMAGE_BUDGET
        {
            let Some(old) = self.entries.pop_front() else {
                break;
            };
            if let Loaded::Image(image) = old.loaded {
                cx.defer(move |cx| cx.drop_image(image, None));
            }
        }
        self.entries.push_back(entry);
    }
}

pub(super) fn source(view: WeakEntity<Breeze>, document: Source, uri: String) -> ImageSource {
    ImageSource::Custom(Arc::new(move |_, cx| {
        view.update(cx, |this, cx| {
            if this.reflow.source.as_ref() != Some(&document) {
                return None;
            }
            if let Some(entry) = this
                .reflow
                .images
                .entries
                .iter()
                .find(|entry| entry.uri == uri)
            {
                return match &entry.loaded {
                    Loaded::Image(image) => Some(Ok(image.clone())),
                    // Nothing is laid out for a failed image; returning an error
                    // without a fallback is how the text view renders nothing.
                    Loaded::Failed => Some(Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                        tr!("Image unavailable")
                    ))))),
                };
            }
            // Another image is already loading; report "not yet" so the text
            // view asks again instead of caching a failure it never saw.
            if this.reflow.images.loading.contains(&uri) {
                return None;
            }
            let Source::File(path) = &document else {
                return None;
            };
            let path = path.clone();
            let source = document.clone();
            this.reflow.images.loading.insert(uri.clone());
            // The loader is called from a `Fn` closure, so the spawned tasks
            // take their own owned copies of the URI.
            let requested = uri.clone();
            let key = uri.clone();
            let work = cx.background_spawn(async move { decode(&path, &requested) });
            cx.spawn(async move |this, cx| {
                let result = work.await;
                let _ = this.update(cx, |this, cx| {
                    if this.reflow.source.as_ref() != Some(&source) {
                        return;
                    }
                    this.reflow.images.loading.remove(&key);
                    let entry = match result {
                        Ok(pixels) => match texture(pixels) {
                            Ok(image) => {
                                let size = image.size(0);
                                Entry {
                                    uri: key,
                                    bytes: usize::from(size.width) * usize::from(size.height) * 4,
                                    loaded: Loaded::Image(image),
                                }
                            }
                            Err(error) => {
                                this.reflow.images.note_failure(&key, &format!("{error:#}"));
                                Entry {
                                    uri: key,
                                    bytes: 0,
                                    loaded: Loaded::Failed,
                                }
                            }
                        },
                        Err(error) => {
                            this.reflow.images.note_failure(&key, &format!("{error:#}"));
                            Entry {
                                uri: key,
                                bytes: 0,
                                loaded: Loaded::Failed,
                            }
                        }
                    };
                    this.reflow.images.insert(cx, entry);
                    this.reflow.list.remeasure();
                    cx.notify();
                });
            })
            .detach();
            None
        })
        .ok()
        .flatten()
    }))
}

impl Images {
    fn note_failure(&mut self, uri: &str, reason: &str) {
        self.failures += 1;
        self.last_failure = Some(format!("{uri}: {reason}"));
    }
}

fn decode(document: &Path, uri: &str) -> Result<Pixels> {
    // The reference is a Markdown URI, so `%20` and a trailing query or
    // fragment are resolved before the file is opened.
    let path = breeze_reflow::markdown::Document::resolve_image(document, uri)?;
    let file = std::fs::File::open(&path)?;
    anyhow::ensure!(
        file.metadata()?.len() <= MAX_IMAGE_BYTES as u64,
        "image exceeds the 16 MiB input limit"
    );
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(IMAGE_BUDGET as u64);
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    reader.limits(limits);
    let image = reader.decode()?.into_rgba8();
    let (width, height) = image.dimensions();
    let mut bgra = image.into_raw();
    anyhow::ensure!(
        bgra.len() <= IMAGE_BUDGET,
        "image exceeds the 32 MiB decoded limit"
    );
    for pixel in bgra.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
        pixel.swap(0, 2);
    }
    Ok(Pixels {
        width,
        height,
        bgra,
    })
}
