//! Local Markdown images are decoded off-thread and retained within a byte budget.
//!
//! A Markdown document may reference remote, missing or unsupported images. A
//! failure is a property of that one image, so it is recorded per URI and
//! rendered as an empty placeholder; it never becomes the reader's status
//! notice, which is reserved for the document as a whole.
use crate::*;
use anyhow::Context as _;
use std::{
    collections::{HashSet, VecDeque},
    io::{Cursor, Read},
    path::Path,
};

const IMAGE_BUDGET: usize = 32 * 1024 * 1024;
const IMAGE_COUNT: usize = 64;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

type CachedLogicalImage = (Arc<RenderImage>, Option<(f32, f32)>);

enum Loaded {
    Image(Arc<RenderImage>),
    Failed,
}

struct Entry {
    uri: String,
    loaded: Loaded,
    bytes: usize,
    logical_size: Option<(f32, f32)>,
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

impl Breeze {
    /// Publish a decoded image (or its failure) and remeasure the reader.
    fn finish_image(
        &mut self,
        cx: &mut Context<Self>,
        key: String,
        result: anyhow::Result<(Pixels, Option<(f32, f32)>)>,
    ) {
        self.reflow.images.loading.remove(&key);
        let entry = match result {
            Ok((pixels, logical_size)) => match texture(pixels) {
                Ok(image) => {
                    let size = image.size(0);
                    Entry {
                        uri: key,
                        bytes: usize::from(size.width) * usize::from(size.height) * 4,
                        loaded: Loaded::Image(image),
                        logical_size,
                    }
                }
                Err(error) => {
                    self.reflow.images.note_failure(&key, &format!("{error:#}"));
                    Entry {
                        uri: key,
                        bytes: 0,
                        loaded: Loaded::Failed,
                        logical_size: None,
                    }
                }
            },
            Err(error) => {
                self.reflow.images.note_failure(&key, &format!("{error:#}"));
                Entry {
                    uri: key,
                    bytes: 0,
                    loaded: Loaded::Failed,
                    logical_size: None,
                }
            }
        };
        self.reflow.images.insert(cx, entry);
        if let Some(markdown) = &self.reflow.markdown {
            let state = markdown.state.clone();
            state.update(cx, |state, cx| state.invalidate_inline_layout(cx));
        }
        cx.notify();
    }
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

    /// The decoded image for `uri`, if it has landed.
    pub fn cached(&self, uri: &str) -> Option<Arc<RenderImage>> {
        self.entries
            .iter()
            .find(|entry| entry.uri == uri)
            .and_then(|entry| match &entry.loaded {
                Loaded::Image(image) => Some(image.clone()),
                Loaded::Failed => None,
            })
    }

    /// The decoded image and its logical display size, if available.
    pub fn cached_with_logical_size(&self, uri: &str) -> Option<CachedLogicalImage> {
        self.entries
            .iter()
            .find(|entry| entry.uri == uri)
            .and_then(|entry| match &entry.loaded {
                Loaded::Image(image) => Some((image.clone(), entry.logical_size)),
                Loaded::Failed => None,
            })
    }

    /// A terminal failure, as opposed to a load still in flight.
    pub fn failed(&self, uri: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.uri == uri && matches!(entry.loaded, Loaded::Failed))
    }

    fn loaded_or_failed(&self, uri: &str) -> bool {
        self.entries.iter().any(|entry| entry.uri == uri)
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
            this.request_image(cx, &document, &uri);
            if let Some(image) = this.reflow.images.cached(&uri) {
                return Some(Ok(image));
            }
            if this.reflow.images.failed(&uri) {
                // Nothing is laid out for a failed image; returning an error
                // without a fallback is how the text view renders nothing.
                return Some(Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(tr!(
                    "Image unavailable"
                ))))));
            }
            // Still loading: report "not yet" so the text view asks again
            // instead of caching a failure it never saw.
            None
        })
        .ok()
        .flatten()
    }))
}

impl Breeze {
    /// Start loading `uri` for the current document unless it is known.
    ///
    /// Native blocks and the rich-text image source share this path, so one
    /// URI is fetched and decoded once.
    pub(super) fn request_image(&mut self, cx: &mut Context<Self>, document: &Source, uri: &str) {
        if self.reflow.images.loaded_or_failed(uri) || self.reflow.images.loading.contains(uri) {
            return;
        }
        let remote = is_remote(uri);
        let path = match document {
            Source::File(path) => Some(path.clone()),
            _ if remote => None,
            _ => return,
        };
        self.reflow.images.loading.insert(uri.to_owned());
        let source = document.clone();
        let requested = uri.to_owned();
        let key = uri.to_owned();
        let work = cx.background_spawn(async move { decode(path.as_deref(), &requested) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.reflow.source.as_ref() != Some(&source) {
                    return;
                }
                this.finish_image(cx, key, result);
            });
        })
        .detach();
    }
}

impl Images {
    fn note_failure(&mut self, uri: &str, reason: &str) {
        self.failures += 1;
        self.last_failure = Some(format!("{uri}: {reason}"));
    }
}

/// Whether the reference is a network image rather than a local file.
fn is_remote(uri: &str) -> bool {
    uri.starts_with("https://") || uri.starts_with("http://")
}

/// Decode a local or remote image, including SVG.
fn decode(document: Option<&Path>, uri: &str) -> Result<(Pixels, Option<(f32, f32)>)> {
    let bytes = if is_remote(uri) {
        fetch_remote(uri)?
    } else {
        let document = document.context("An image needs a document directory")?;
        // The reference is a Markdown URI, so `%20` and a trailing query or
        // fragment are resolved before the file is opened.
        let path = breeze_reflow::markdown::resolve_image(document, uri)?;
        let file = std::fs::File::open(&path)?;
        anyhow::ensure!(
            file.metadata()?.len() <= MAX_IMAGE_BYTES as u64,
            "image exceeds the 16 MiB input limit"
        );
        let mut bytes = Vec::new();
        file.take(MAX_IMAGE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        bytes
    };
    if looks_like_svg(&bytes) {
        let logical_size = svg_logical_size(&bytes)?;
        Ok((rasterize_svg(&bytes, 2.0)?, Some(logical_size)))
    } else {
        Ok((decode_raster(bytes)?, None))
    }
}

/// A bounded network fetch. Badges and screenshots are small; a document must
/// not be able to block the reader on an arbitrarily large response.
fn fetch_remote(uri: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(uri.len() <= 2048, "image URL is too long");
    let config = ureq::Agent::config_builder()
        .https_only(false)
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .max_redirects(5)
        .http_status_as_error(true)
        .user_agent(concat!("Breeze/", env!("CARGO_PKG_VERSION")));
    // A loopback fixture must not use a developer machine's proxy settings.
    #[cfg(test)]
    let config = config.proxy(None);
    let agent: ureq::Agent = config.build().into();
    let mut response = agent.get(uri).call()?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_IMAGE_BYTES as u64)
        .read_to_vec()?;
    anyhow::ensure!(!bytes.is_empty(), "the image response is empty");
    Ok(bytes)
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(512)];
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start();
    text.starts_with("<svg") || text.starts_with("<?xml") && text.contains("<svg")
}

fn svg_logical_size(bytes: &[u8]) -> Result<(f32, f32)> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(bytes, &options)?;
    let size = tree.size();
    anyhow::ensure!(size.width() > 0. && size.height() > 0., "empty SVG");
    Ok((size.width(), size.height()))
}

/// Rasterize an SVG for the shared TextView image path.
pub(super) fn rasterize_svg(bytes: &[u8], scale: f32) -> Result<Pixels> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(bytes, &options)?;
    let size = tree.size();
    anyhow::ensure!(size.width() > 0. && size.height() > 0., "empty SVG");
    let scale = scale.clamp(0.5, 3.0);
    let width = (size.width() * scale).ceil() as u32;
    let height = (size.height() * scale).ceil() as u32;
    anyhow::ensure!(
        width <= 8192 && height <= 8192,
        "SVG exceeds the 8192 px limit"
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .context("the SVG is larger than the pixel budget")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut bgra = pixmap.take();
    anyhow::ensure!(
        bgra.len() <= IMAGE_BUDGET,
        "image exceeds the 32 MiB decoded limit"
    );
    // `tiny-skia` produces premultiplied RGBA; the texture path stores the
    // channel order the raster decoder produces.
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Pixels {
        width,
        height,
        bgra,
    })
}

/// Rasterize an SVG and turn it into the reader's texture format.
pub(super) fn svg_image(bytes: &[u8], scale: f32) -> Result<Arc<RenderImage>> {
    texture(rasterize_svg(bytes, scale)?)
}

fn decode_raster(bytes: Vec<u8>) -> Result<Pixels> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write as _, net::TcpListener};

    #[test]
    fn svg_detection_is_conservative() {
        assert!(looks_like_svg(
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"
        ));
        assert!(looks_like_svg(
            b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"
        ));
        assert!(!looks_like_svg(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn a_remote_svg_is_fetched_and_rasterized() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="16"><rect width="40" height="16" fill="#333"/></svg>"##;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let uri = format!("http://{address}/badge.svg");
        let bytes = fetch_remote(&uri).expect("the SVG is fetched");
        let pixels = rasterize_svg(&bytes, 2.0).expect("the SVG is rasterized");
        assert_eq!((pixels.width, pixels.height), (80, 32));
        assert_eq!(pixels.bgra.len(), 80 * 32 * 4);
        server.join().unwrap();
    }
}
