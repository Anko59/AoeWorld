//! Gzip for the static file services. A compressible file is encoded the first
//! time a request that allows gzip asks for it by its plain path; the encoded
//! bytes are then sent for as long as the file keeps its `Last-Modified` and
//! `Content-Length`. Every other response leaves as the file service made it.
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Request, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{
            ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE,
            LAST_MODIFIED, RANGE, VARY,
        },
    },
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
};
use flate2::{Compression, write::GzEncoder};
use futures_util::StreamExt;
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::Semaphore;
use tower_http::services::ServeDir;

/// A smaller file is sent as it is: there is too little to save.
const MIN_LENGTH: u32 = 1024;
/// A larger file is streamed as it is rather than held in memory to encode.
const MAX_LENGTH: u32 = 16 * 1024 * 1024;
/// Encoded bytes kept, counted with their keys and bookkeeping.
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
/// File bytes held for encoding at once; a request past that gets its file raw.
const MAX_ENCODING_BYTES: usize = 2 * MAX_LENGTH as usize;

/// A file as a request names it: where its directory is served, then the path
/// asked for under it. Nesting strips the first from the request, and two
/// directories may hold a file of the same name, date and length.
type Key = (&'static str, String);

/// What tells one content of a file from the next.
#[derive(PartialEq)]
struct Version {
    modified: HeaderValue,
    length: u32,
}

struct Entry {
    version: Version,
    /// The gzip body, or nothing when gzip does not make that version smaller.
    encoded: Bytes,
    resident_bytes: usize,
    last_used: u64,
}

#[derive(Default)]
struct Entries {
    by_key: BTreeMap<Key, Entry>,
    resident_bytes: usize,
    clock: u64,
}

/// The gzip bodies of static files, and the count of the encoder's runs.
pub(crate) struct Cache {
    entries: Mutex<Entries>,
    limit: usize,
    encoding: Arc<Semaphore>,
    compressions: AtomicU64,
}

impl Default for Cache {
    fn default() -> Self {
        Self::with_limit(MAX_CACHE_BYTES)
    }
}

impl Cache {
    /// A cache that keeps at most `limit` bytes.
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            entries: Mutex::default(),
            limit,
            encoding: Arc::new(Semaphore::new(MAX_ENCODING_BYTES)),
            compressions: AtomicU64::new(0),
        }
    }

    /// Responses encoded so far: one per run of the encoder, none per hit.
    pub(crate) fn compressions(&self) -> u64 {
        self.compressions.load(Ordering::Relaxed)
    }

    /// `files` behind the gzip layer, for a router that serves them at `mount`.
    pub(crate) fn serve(self: &Arc<Self>, mount: &'static str, files: ServeDir) -> Router {
        let scope = Scope {
            cache: Arc::clone(self),
            mount,
        };
        Router::new()
            .fallback_service(files)
            .layer(from_fn_with_state(scope, encode))
    }

    /// `router` with `files` served under `mount`, behind the gzip layer.
    pub(crate) fn nest(
        self: &Arc<Self>,
        router: Router,
        mount: &'static str,
        files: ServeDir,
    ) -> Router {
        router.nest_service(mount, self.serve(mount, files))
    }

    fn lock(&self) -> MutexGuard<'_, Entries> {
        self.entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// The encoded bytes kept for `key`, if they were made from `version`.
    fn get(&self, key: &Key, version: &Version) -> Option<Bytes> {
        let mut entries = self.lock();
        entries.clock = entries.clock.saturating_add(1);
        let last_used = entries.clock;
        let entry = entries.by_key.get_mut(key)?;
        if entry.version != *version {
            return None;
        }
        entry.last_used = last_used;
        Some(entry.encoded.clone())
    }

    /// Keeps `encoded` for `key` in place of what an older version left, then
    /// drops the least recently used entries until the cap holds again.
    fn insert(&self, key: Key, version: Version, encoded: Bytes) {
        // Charge the encoded bytes, the path, the date, the entry and
        // conservative BTree node slack: many paths can name one small file.
        let resident_bytes = encoded.len()
            + key.1.capacity()
            + version.modified.len()
            + size_of::<(Key, Entry)>()
            + 4 * size_of::<usize>();
        let mut entries = self.lock();
        if let Some(previous) = entries.by_key.remove(&key) {
            entries.resident_bytes = entries
                .resident_bytes
                .saturating_sub(previous.resident_bytes);
        }
        if resident_bytes > self.limit {
            return;
        }
        entries.clock = entries.clock.saturating_add(1);
        let last_used = entries.clock;
        entries.resident_bytes = entries.resident_bytes.saturating_add(resident_bytes);
        entries.by_key.insert(
            key,
            Entry {
                version,
                encoded,
                resident_bytes,
                last_used,
            },
        );
        while entries.resident_bytes > self.limit {
            let oldest = entries
                .by_key
                .iter()
                .min_by_key(|(_, entry)| entry.last_used);
            let Some(oldest) = oldest.map(|(key, _)| key.clone()) else {
                break;
            };
            if let Some(entry) = entries.by_key.remove(&oldest) {
                entries.resident_bytes =
                    entries.resident_bytes.saturating_sub(entry.resident_bytes);
            }
        }
    }

    /// Gzips `file`, keeps the result for `key` and counts the run. The result
    /// is the gzip body, or an empty one when gzip does not make this version
    /// smaller: the file is then sent as it is and not encoded again. Runs on
    /// a blocking thread, to its end even when the request that asked is gone.
    fn gzip(&self, key: Key, version: Version, file: &[u8]) -> io::Result<Bytes> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(file)?;
        let encoded = encoder.finish()?;
        let encoded = if encoded.len() < file.len() {
            Bytes::from(encoded.into_boxed_slice())
        } else {
            Bytes::new()
        };
        self.insert(key, version, encoded.clone());
        self.compressions.fetch_add(1, Ordering::Relaxed);
        Ok(encoded)
    }
}

/// The cache as one served directory uses it.
#[derive(Clone)]
struct Scope {
    cache: Arc<Cache>,
    mount: &'static str,
}

/// Answers a `GET` for the plain path of a compressible file with its gzip
/// body, kept from an earlier request or made now. A request that does not
/// allow gzip, or that finds the encoder at its budget, gets the file itself,
/// marked as one of two representations; so does every request for a file that
/// gzip does not make smaller.
async fn encode(State(scope): State<Scope>, request: Request, next: Next) -> Response {
    let untouched = request.method() != Method::GET
        || request.headers().contains_key(RANGE)
        || !plain(request.uri().path());
    if untouched {
        return next.run(request).await;
    }
    let allowed = accepts_gzip(request.headers());
    let uri = request.uri().clone();
    let response = next.run(request).await;
    let Some(version) = encodable(&response, uri.path()) else {
        return response;
    };
    let (mut parts, body) = response.into_parts();
    let varies = HeaderValue::from_static("Accept-Encoding");
    parts.headers.append(VARY, varies);
    if !allowed {
        return Response::from_parts(parts, body);
    }
    let (mount, path) = (scope.mount, uri.path());
    let key = (mount, path.to_owned());
    // A hit leaves the file unread: `body` is passed on or dropped as it came.
    let (encoded, body) = if let Some(encoded) = scope.cache.get(&key, &version) {
        (encoded, body)
    } else {
        let length = version.length;
        let budget = Arc::clone(&scope.cache.encoding).try_acquire_many_owned(length);
        let Ok(budget) = budget else {
            return Response::from_parts(parts, body);
        };
        let Some(file) = read(body, length as usize).await else {
            tracing::warn!(mount, path, "static file changed while it was read");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        // The encoder owns all it needs to finish, keep and count its run:
        // a request that is gone before it ends does not waste it.
        let cache = Arc::clone(&scope.cache);
        let task = tokio::task::spawn_blocking(move || {
            let _budget = budget;
            let encoded = cache.gzip(key, version, &file);
            encoded.map(|encoded| (encoded, file))
        });
        let Ok(Ok((encoded, file))) = task.await else {
            tracing::warn!(mount, path, "static file could not be encoded");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        (encoded, Body::from(file))
    };
    if encoded.is_empty() {
        // Gzip does not make this version smaller: the file is the reply.
        return Response::from_parts(parts, body);
    }
    let coding = HeaderValue::from_static("gzip");
    parts.headers.insert(CONTENT_ENCODING, coding);
    parts.headers.insert(CONTENT_LENGTH, encoded.len().into());
    // Ranges are only served from the file itself.
    parts.headers.remove(ACCEPT_RANGES);
    Response::from_parts(parts, Body::from(encoded))
}

/// Whether `path` is the plain spelling of what it names, the only one that is
/// encoded and kept. `ServeDir` undoes percent escapes and skips empty and `.`
/// segments, so a file has as many other spellings as a client cares to send;
/// each of those gets the file as `ServeDir` answers it, at no cost of an
/// encoder run or an entry. A trailing slash is plain: it names an index page.
fn plain(path: &str) -> bool {
    let Some(inner) = path.strip_prefix('/') else {
        return false;
    };
    let inner = inner.strip_suffix('/').unwrap_or(inner);
    let odd = |segment: &str| matches!(segment, "" | ".");
    path == "/" || !(path.contains('%') || inner.split('/').any(odd))
}

/// The version of the file in `response`, if this layer may encode it: a
/// complete `200` that is not encoded yet, of a compressible type, dated, and
/// of a size worth holding in memory.
fn encodable(response: &Response, path: &str) -> Option<Version> {
    let headers = response.headers();
    if response.status() != StatusCode::OK || headers.contains_key(CONTENT_ENCODING) {
        return None;
    }
    let length = headers.get(CONTENT_LENGTH)?.to_str().ok()?.parse().ok()?;
    let kind = headers.get(CONTENT_TYPE)?.to_str().ok()?;
    let modified = headers.get(LAST_MODIFIED)?.clone();
    ((MIN_LENGTH..=MAX_LENGTH).contains(&length) && compressible(kind, path))
        .then_some(Version { modified, length })
}

/// Whether gzip pays for this type. `ServeDir` knows no type for WGSL and
/// calls it `application/octet-stream`, as it does the pack's `.bin` chunks,
/// whose `Content-Length` the client checks against the chunk: there only the
/// extension tells the shader text apart.
fn compressible(kind: &str, path: &str) -> bool {
    match kind.split(';').next().unwrap_or(kind).trim() {
        "application/wasm" | "application/javascript" | "application/json" | "image/svg+xml" => {
            true
        }
        "application/octet-stream" => path
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("wgsl")),
        kind => kind.starts_with("text/"),
    }
}

/// Whether `Accept-Encoding` allows a gzip coding: a weight above zero on
/// `gzip` (or its older name `x-gzip`), else on `*`. A malformed weight counts
/// as zero, and without the header only the file itself is allowed.
fn accepts_gzip(headers: &HeaderMap) -> bool {
    let (mut named, mut any) = (None, None);
    for value in headers.get_all(ACCEPT_ENCODING) {
        let Ok(value) = value.to_str() else {
            return false;
        };
        for entry in value.split(',') {
            let mut fields = entry.split(';').map(str::trim);
            let coding = fields.next().unwrap_or_default();
            let gzip = ["gzip", "x-gzip"]
                .iter()
                .any(|name| coding.eq_ignore_ascii_case(name));
            let seen = match coding {
                _ if gzip => &mut named,
                "*" => &mut any,
                _ => continue,
            };
            let weight = fields.find_map(|field| {
                let (name, weight) = field.split_once('=')?;
                name.trim()
                    .eq_ignore_ascii_case("q")
                    .then_some(weight.trim())
            });
            let positive = weight.is_none_or(|weight| {
                weight
                    .parse::<f32>()
                    .is_ok_and(|weight| weight > 0.0 && weight <= 1.0)
            });
            *seen = Some(positive && seen.unwrap_or(true));
        }
    }
    named.or(any).unwrap_or(false)
}

/// Reads the `length` bytes the response announced out of `body`, no more than
/// the connection would have sent. `None` when the read fails or ends short:
/// the file changed under it.
async fn read(body: Body, length: usize) -> Option<Vec<u8>> {
    let mut chunks = body.into_data_stream();
    let mut file = Vec::with_capacity(length);
    while file.len() < length {
        let chunk = chunks.next().await?.ok()?;
        let wanted = chunk.len().min(length - file.len());
        file.extend_from_slice(&chunk[..wanted]);
    }
    Some(file)
}

#[cfg(test)]
mod tests;
