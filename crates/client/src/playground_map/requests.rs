//! Bounded camera-demand window; retired completions never own replacement slots.
//! Handles are browser-neutral so lifecycle races have native regression tests.
pub(crate) const MAX_REQUESTED_CHUNKS: usize = 64;
type Coordinate = (i32, i32);

#[derive(Clone)]
pub(crate) struct Request<T> {
    pub coordinate: Coordinate,
    pub id: u64,
    pub handle: T,
}

pub(crate) struct Update<T> {
    pub started: Vec<Request<T>>,
    pub cancelled: Vec<T>,
}

pub(crate) struct RequestWindow<T> {
    next_id: u64,
    pending: Vec<Request<T>>,
}

impl<T> Default for RequestWindow<T> {
    fn default() -> Self {
        Self {
            next_id: 0,
            pending: Vec::new(),
        }
    }
}

impl<T> RequestWindow<T> {
    pub fn complete(&mut self, coordinate: Coordinate, id: u64) -> bool {
        let Some(index) = self
            .pending
            .iter()
            .position(|request| request.coordinate == coordinate && request.id == id)
        else {
            return false;
        };
        self.pending.swap_remove(index);
        true
    }

    pub fn clear(&mut self) -> Vec<T> {
        // Never reset next_id: a reconnect to the same map can reuse coordinates.
        std::mem::take(&mut self.pending)
            .into_iter()
            .map(|request| request.handle)
            .collect()
    }
}

impl<T: Clone> RequestWindow<T> {
    pub fn reconcile(
        &mut self,
        demanded: &[Coordinate],
        can_cancel: impl Fn(&T) -> bool,
        mut make_handle: impl FnMut() -> T,
    ) -> Update<T> {
        // At most64 slots: bounded scans avoid another tree allocation and
        // browser WASM tree monomorphization for per-request handles.
        let wanted = &demanded[..demanded.len().min(MAX_REQUESTED_CHUNKS)];
        let mut cancelled = Vec::new();
        self.pending.retain(|request| {
            if !wanted.contains(&request.coordinate) && can_cancel(&request.handle) {
                cancelled.push(request.handle.clone());
                false
            } else {
                // A browser without cancellation support retains its slots until
                // completion; dropping bookkeeping alone would violate the cap.
                true
            }
        });
        let mut started = Vec::new();
        for &coordinate in demanded.iter().take(MAX_REQUESTED_CHUNKS) {
            if self.pending.len() == MAX_REQUESTED_CHUNKS {
                break;
            }
            if self
                .pending
                .iter()
                .any(|request| request.coordinate == coordinate)
            {
                continue;
            }
            self.next_id = self.next_id.wrapping_add(1);
            let request = Request {
                coordinate,
                id: self.next_id,
                handle: make_handle(),
            };
            self.pending.push(request.clone());
            started.push(request);
        }
        Update { started, cancelled }
    }
}

#[path = "requests/tests.rs"]
#[cfg(test)]
mod tests;
