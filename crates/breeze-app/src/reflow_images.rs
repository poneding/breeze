//! Local Markdown images are decoded off-thread and retained within a byte budget.
use crate::*;
use std::{
    collections::VecDeque,
    io::{Cursor, Read},
    path::Path,
};

const IMAGE_BUDGET: usize = 32 * 1024 * 1024;
struct Entry {
    uri: String,
    image: Option<Arc<RenderImage>>,
    bytes: usize,
}
#[derive(Default)]
pub(super) struct Images {
    entries: VecDeque<Entry>,
    busy: bool,
}
impl Images {
    pub fn clear(&mut self, cx: &mut App) {
        self.busy = false;
        for entry in self.entries.drain(..) {
            if let Some(image) = entry.image {
                cx.defer(move |cx| cx.drop_image(image, None));
            }
        }
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
                return entry.image.clone().map(Ok);
            }
            if this.reflow.images.busy {
                return None;
            }
            let Source::File(path) = &document else {
                return None;
            };
            let parent = path.parent()?;
            let parent = parent.to_path_buf();
            let uri = uri.clone();
            let source = document.clone();
            this.reflow.images.busy = true;
            let requested = uri.clone();
            let work = cx.background_spawn(async move { decode(&parent, &requested) });
            cx.spawn(async move |this, cx| {
                let result = work.await;
                let _ = this.update(cx, |this, cx| {
                    if this.reflow.source.as_ref() != Some(&source) {
                        return;
                    }
                    this.reflow.images.busy = false;
                    let image = result.and_then(texture);
                    let (image, bytes) = match image {
                        Ok(image) => {
                            let size = image.size(0);
                            let bytes = usize::from(size.width) * usize::from(size.height) * 4;
                            (Some(image), bytes)
                        }
                        Err(error) => {
                            this.message = tf!("Image unavailable: {}", format!("{error:#}"));
                            (None, 0)
                        }
                    };
                    while this.reflow.images.entries.len() >= 32
                        || this
                            .reflow
                            .images
                            .entries
                            .iter()
                            .map(|entry| entry.bytes)
                            .sum::<usize>()
                            + bytes
                            > IMAGE_BUDGET
                    {
                        if let Some(entry) = this.reflow.images.entries.pop_front() {
                            if let Some(image) = entry.image {
                                cx.defer(move |cx| cx.drop_image(image, None));
                            }
                        } else {
                            break;
                        }
                    }
                    this.reflow
                        .images
                        .entries
                        .push_back(Entry { uri, image, bytes });
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

fn decode(parent: &Path, uri: &str) -> Result<Pixels> {
    anyhow::ensure!(
        !uri.contains(':') && !uri.starts_with("//") && !Path::new(uri).is_absolute(),
        "Only relative local Markdown images are supported"
    );
    let path = parent.join(uri);
    let file = std::fs::File::open(&path)?;
    anyhow::ensure!(
        file.metadata()?.len() <= 16 * 1024 * 1024,
        "Image exceeds the 16 MiB input limit"
    );
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
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
        "Image exceeds the 32 MiB decoded limit"
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
