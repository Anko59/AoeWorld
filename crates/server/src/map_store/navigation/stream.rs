//! Bounded chunk production: four worker caches, eight encoded messages and one
//! writer. Neither preparation nor publication retains a whole-map chunk Vec.
use super::*;
use std::{
    io::{BufWriter, Seek, SeekFrom, Write},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

pub(super) struct Writer {
    path: PathBuf,
    temporary: PathBuf,
    data: BufWriter<fs::File>,
    index: fs::File,
    next: u64,
    complete: usize,
    count: usize,
    limit: u64,
}
impl Writer {
    pub(super) fn new(
        path: &Path,
        hash: [u8; 32],
        side: usize,
        limit: u64,
    ) -> Result<Self, String> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let count = side
            .checked_mul(side)
            .ok_or("navigation storage budget exceeded")?;
        let next = (HEADER_BYTES
            + count
                .checked_mul(INDEX_RECORD_BYTES)
                .ok_or("navigation storage budget exceeded")?) as u64;
        if next > limit {
            return Err("navigation storage budget exceeded".into());
        }
        let temporary = path.with_extension(format!(
            "{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut data = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            data.write_all(MAGIC)?;
            data.write_all(&hash)?;
            data.write_all(&(side as u32).to_le_bytes())?;
            data.set_len(next)?;
            data.seek(SeekFrom::Start(next))?;
            let index = fs::OpenOptions::new().write(true).open(&temporary)?;
            Ok::<_, std::io::Error>((data, index))
        })();
        let (data, index) = result.map_err(|e| {
            let _ = fs::remove_file(&temporary);
            e.to_string()
        })?;
        Ok(Self {
            path: path.into(),
            temporary,
            data: BufWriter::with_capacity(1 << 20, data),
            index,
            next,
            complete: 0,
            count,
            limit,
        })
    }
    pub(super) fn push(&mut self, at: usize, bytes: &[u8]) -> Result<(), String> {
        if at >= self.count
            || bytes.len() > MAX_CHUNK_BYTES
            || self.next + bytes.len() as u64 > self.limit
        {
            return Err("navigation storage budget exceeded".into());
        }
        let result = (|| {
            self.index.seek(SeekFrom::Start(
                (HEADER_BYTES + at * INDEX_RECORD_BYTES) as u64,
            ))?;
            self.index.write_all(&self.next.to_le_bytes())?;
            self.index.write_all(&(bytes.len() as u32).to_le_bytes())?;
            self.index.write_all(&aoe_map::navigation_checksum(bytes))?;
            self.data.write_all(bytes)
        })();
        result.map_err(|e| e.to_string())?;
        self.next += bytes.len() as u64;
        self.complete += 1;
        Ok(())
    }
    pub(super) fn finish(mut self) -> Result<(), String> {
        if self.complete != self.count {
            return Err("incomplete static navigation".into());
        }
        (|| {
            self.data.flush()?;
            let mut permissions = self.data.get_ref().metadata()?.permissions();
            permissions.set_readonly(true);
            self.data.get_ref().set_permissions(permissions)?;
            self.data.get_ref().sync_all()?;
            fs::rename(&self.temporary, &self.path)?;
            fs::File::open(
                self.path
                    .parent()
                    .ok_or_else(|| std::io::Error::other("navigation path has no parent"))?,
            )?
            .sync_all()
        })()
        .map_err(|e: std::io::Error| e.to_string())
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.temporary);
    }
}

pub(super) fn build(
    generator: &MapChunkGenerator,
    side: usize,
    workers: usize,
    path: &Path,
    hash: [u8; 32],
    limit: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    let mut writer = Writer::new(path, hash, side, limit)?;
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let (send, receive) = mpsc::sync_channel(workers * 2);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (next, stop, send) = (&next, &stop, send.clone());
            let worker = generator.navigation_preparation_worker();
            scope.spawn(move || {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    if at >= side * side || stop.load(Ordering::Acquire) {
                        break;
                    }
                    let bytes = worker
                        .prepare_static_navigation_chunk(
                            (at % side) as i32,
                            (at / side) as i32,
                            &|| stop.load(Ordering::Acquire),
                        )
                        .and_then(|chunk| chunk.encode())
                        .map_err(|e| e.to_string());
                    if send.send((at, bytes)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);
        let mut error = None;
        let mut last_progress = Instant::now();
        loop {
            if cancelled() {
                stop.store(true, Ordering::Release);
                error.get_or_insert_with(|| "navigation preparation cancelled".to_owned());
            }
            match receive.recv_timeout(Duration::from_millis(100)) {
                Ok((at, Ok(bytes))) if error.is_none() => {
                    if let Err(reason) = writer.push(at, &bytes) {
                        stop.store(true, Ordering::Release);
                        error = Some(reason);
                    }
                }
                Ok((_, Ok(_))) => {}
                Ok((_, Err(reason))) => {
                    stop.store(true, Ordering::Release);
                    error.get_or_insert(reason);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if last_progress.elapsed() >= Duration::from_secs(10) {
                tracing::info!(
                    complete = writer.complete,
                    total = side * side,
                    file_bytes = writer.next,
                    "streaming static navigation"
                );
                last_progress = Instant::now();
            }
        }
        if let Some(error) = error {
            return Err(error);
        }
        writer.finish()
    })
}
