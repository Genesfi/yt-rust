//! Custom egui `BytesLoader` and `TextureLoader` modeled after `ytm-rs`:
//! reuses the backend's pooled `reqwest::Client`, bounds concurrent image
//! fetches, persists a trimmed disk cache (`thumbs/`), retries transient errors,
//! and evicts offscreen GPU textures beyond a memory budget.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::load::{
    Bytes, BytesLoadResult, BytesLoader, BytesPoll, ImagePoll, LoadError, SizeHint, SizedTexture,
    TextureLoadResult, TextureLoader, TexturePoll,
};
use egui::{TextureHandle, TextureOptions, Vec2};

use crate::backend::Backend;

const RETRY_AFTER: Duration = Duration::from_secs(30);
const DISK_MAX_FILE: usize = 256 * 1024;
const DISK_BUDGET: u64 = 64 * 1024 * 1024;
const MAX_DOWNLOADS: usize = 8;
const BYTES_BUDGET: usize = 32 * 1024 * 1024;
const TEXTURE_BUDGET: usize = 64 * 1024 * 1024;
const BIG: usize = 1024 * 1024;
const BIG_UNUSED: Duration = Duration::from_secs(30);

enum Entry {
    Pending,
    Ready(Arc<[u8]>, Option<String>),
    Failed(String, Instant),
}

fn is_http(uri: &str) -> bool {
    uri.starts_with("https://") || uri.starts_with("http://")
}

pub struct HttpImages {
    backend: Backend,
    cache: Arc<Mutex<HashMap<String, Entry>>>,
    dir: std::path::PathBuf,
    turns: Arc<tokio::sync::Semaphore>,
}

impl HttpImages {
    pub fn new(backend: Backend) -> Self {
        let dir = backend.data_dir().join("thumbs");
        let _ = std::fs::create_dir_all(&dir);
        trim(&dir);
        Self {
            backend,
            cache: Default::default(),
            dir,
            turns: Arc::new(tokio::sync::Semaphore::new(MAX_DOWNLOADS)),
        }
    }
}

fn store(cache: &Mutex<HashMap<String, Entry>>, uri: &str, entry: Entry) {
    let mut cache = cache.lock().unwrap();
    let Some(slot) = cache.get_mut(uri) else {
        return;
    };
    *slot = entry;
    if cache.len() > 512 {
        cache.retain(|k, e| match e {
            Entry::Failed(_, at) if k != uri && at.elapsed() >= RETRY_AFTER => false,
            _ => true,
        });
    }
    let mut total: usize = cache
        .values()
        .map(|e| if let Entry::Ready(b, _) = e { b.len() } else { 0 })
        .sum();
    if total <= BYTES_BUDGET {
        return;
    }
    cache.retain(|k, e| {
        let Entry::Ready(b, _) = e else {
            return true;
        };
        if total <= BYTES_BUDGET || k == uri {
            return true;
        }
        total -= b.len();
        false
    });
}

fn disk_path(dir: &std::path::Path, uri: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    uri.hash(&mut h);
    dir.join(format!("{:016x}", h.finish()))
}

fn trim(dir: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = rd
        .flatten()
        .filter_map(|f| {
            let m = f.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), f.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, path) in files {
        if total <= DISK_BUDGET {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total -= len;
        }
    }
}

impl BytesLoader for HttpImages {
    fn id(&self) -> &str {
        egui::generate_loader_id!(HttpImages)
    }

    fn load(&self, _ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !is_http(uri) {
            return Err(LoadError::NotSupported);
        }
        let mut cache = self.cache.lock().unwrap();
        match cache.get(uri) {
            Some(Entry::Ready(bytes, mime)) => {
                return Ok(BytesPoll::Ready {
                    size: None,
                    bytes: Bytes::Shared(bytes.clone()),
                    mime: mime.clone(),
                });
            }
            Some(Entry::Pending) => return Ok(BytesPoll::Pending { size: None }),
            Some(Entry::Failed(e, at)) if at.elapsed() < RETRY_AFTER => {
                return Err(LoadError::Loading(e.clone()));
            }
            Some(Entry::Failed(..)) | None => {}
        }
        cache.insert(uri.to_owned(), Entry::Pending);
        drop(cache);

        let (http, cache, uri, backend) = (
            self.backend.http(),
            self.cache.clone(),
            uri.to_owned(),
            self.backend.clone(),
        );
        let turns = self.turns.clone();
        let file = disk_path(&self.dir, &uri);
        self.backend.spawn_task(async move {
            if let Ok(bytes) = std::fs::read(&file) {
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&file)
                    .and_then(|f| f.set_modified(std::time::SystemTime::now()));
                store(&cache, &uri, Entry::Ready(Arc::from(bytes), None));
                backend.wake();
                return;
            }
            let res = async {
                let _turn = turns.acquire().await;
                let resp = http.get(&uri).send().await?.error_for_status()?;
                let mime = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                Ok::<_, reqwest::Error>((resp.bytes().await?, mime))
            }
            .await;
            let entry = match res {
                Ok((bytes, mime)) => {
                    if bytes.len() <= DISK_MAX_FILE {
                        let _ = std::fs::write(&file, &bytes);
                    }
                    Entry::Ready(Arc::from(&bytes[..]), mime)
                }
                Err(e) => Entry::Failed(e.to_string(), Instant::now()),
            };
            store(&cache, &uri, entry);
            backend.wake();
        });
        Ok(BytesPoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        self.cache.lock().unwrap().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().unwrap().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .unwrap()
            .values()
            .map(|e| match e {
                Entry::Ready(b, _) => b.len(),
                _ => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.cache
            .lock()
            .unwrap()
            .values()
            .any(|e| matches!(e, Entry::Pending))
    }
}

struct Texture {
    handle: TextureHandle,
    options: TextureOptions,
    size: Vec2,
    last_used: u64,
    drawn: Instant,
}

#[derive(Default)]
struct TextureState {
    pass: u64,
    cache: HashMap<String, Texture>,
}

/// Bounded GPU texture cache that releases CPU decoded copies immediately after upload.
#[derive(Default)]
pub struct Textures {
    state: Mutex<TextureState>,
}

impl TextureLoader for Textures {
    fn id(&self) -> &str {
        egui::generate_loader_id!(Textures)
    }

    fn load(
        &self,
        ctx: &egui::Context,
        uri: &str,
        options: TextureOptions,
        size_hint: SizeHint,
    ) -> TextureLoadResult {
        if !is_http(uri) {
            return Err(LoadError::NotSupported);
        }
        {
            let mut state = self.state.lock().unwrap();
            let pass = state.pass;
            if let Some(t) = state.cache.get_mut(uri).filter(|t| t.options == options) {
                t.last_used = pass;
                t.drawn = Instant::now();
                return Ok(TexturePoll::Ready {
                    texture: SizedTexture::new(t.handle.id(), t.size),
                });
            }
        }
        match ctx.try_load_image(uri, size_hint) {
            Err(e) => {
                for l in ctx.loaders().image.lock().iter() {
                    l.forget(uri);
                }
                Err(e)
            }
            Ok(ImagePoll::Pending { size }) => Ok(TexturePoll::Pending { size }),
            Ok(ImagePoll::Ready { image }) => {
                let size = image.source_size;
                let handle = ctx.load_texture(uri, image, options);
                let texture = SizedTexture::new(handle.id(), size);
                let loaders = ctx.loaders();
                for l in loaders.bytes.lock().iter() {
                    l.forget(uri);
                }
                for l in loaders.image.lock().iter() {
                    l.forget(uri);
                }
                let mut state = self.state.lock().unwrap();
                let last_used = state.pass;
                state.cache.insert(
                    uri.to_owned(),
                    Texture {
                        handle,
                        options,
                        size,
                        last_used,
                        drawn: Instant::now(),
                    },
                );
                Ok(TexturePoll::Ready { texture })
            }
        }
    }

    fn forget(&self, uri: &str) {
        self.state.lock().unwrap().cache.remove(uri);
    }

    fn forget_all(&self) {
        self.state.lock().unwrap().cache.clear();
    }

    fn end_pass(&self, pass_index: u64) {
        let mut state = self.state.lock().unwrap();
        state.pass = pass_index;
        state
            .cache
            .retain(|_, t| t.handle.byte_size() < BIG || t.drawn.elapsed() < BIG_UNUSED);
        let mut total = state
            .cache
            .values()
            .map(|t| t.handle.byte_size())
            .sum::<usize>();
        if total <= TEXTURE_BUDGET {
            return;
        }
        let mut old: Vec<(u64, String)> = state
            .cache
            .iter()
            .filter(|(_, t)| t.last_used + 2 < pass_index)
            .map(|(k, t)| (t.last_used, k.clone()))
            .collect();
        old.sort_unstable();
        for (_, uri) in old {
            if total <= TEXTURE_BUDGET * 3 / 4 {
                break;
            }
            if let Some(t) = state.cache.remove(&uri) {
                total -= t.handle.byte_size();
            }
        }
    }

    fn byte_size(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .cache
            .values()
            .map(|t| t.handle.byte_size())
            .sum()
    }
}
