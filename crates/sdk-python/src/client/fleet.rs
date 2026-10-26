use super::*;

/// Many connections served by one thread: `wait` blocks (GIL released) until
/// at least one has a decision due and returns every due one. One Python
/// process can drive 100 bots from one loop without a thread each.
#[pyclass(frozen, module = "aoeworld")]
pub struct Fleet {
    clients: Vec<Arc<aoe_sdk::Client>>,
    /// Indices whose closure was already reported: they are never waited on
    /// again, so a closed connection cannot make `wait` return at once forever.
    closed: std::sync::Mutex<std::collections::BTreeSet<usize>>,
}

#[pymethods]
impl Fleet {
    #[new]
    fn new(clients: Vec<PyRef<'_, Client>>) -> Self {
        Self {
            clients: clients.iter().map(|client| client.inner.clone()).collect(),
            closed: std::sync::Mutex::default(),
        }
    }

    fn __len__(&self) -> usize {
        self.clients.len()
    }

    /// `[(index, decision)]` for every connection with a decision due, in
    /// list order; `decision` is `None` for a connection that has closed,
    /// reported exactly once, after which the fleet ignores it. Returns an
    /// empty list on timeout (also when every connection has closed: the call
    /// still waits out the timeout, so a loop around it cannot spin).
    #[pyo3(signature = (timeout=None))]
    fn wait(
        &self,
        py: Python<'_>,
        timeout: Option<f64>,
    ) -> PyResult<Vec<(usize, Option<Decision>)>> {
        use futures_util::FutureExt;
        let closed = self
            .closed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let clients: Vec<(usize, Arc<aoe_sdk::Client>)> = self
            .clients
            .iter()
            .cloned()
            .enumerate()
            .filter(|(index, _)| !closed.contains(index))
            .collect();
        let limit = timeout.map(|seconds| Duration::from_secs_f64(seconds.max(0.0)));
        let rt = runtime()?;
        let due = py.allow_threads(move || {
            rt.block_on(async move {
                let first = async {
                    if clients.is_empty() {
                        // Nothing left to hear from: idle for the caller's timeout.
                        return std::future::pending().await;
                    }
                    let waits = clients.iter().map(|(index, client)| {
                        let index = *index;
                        Box::pin(async move { (index, client.next_decision().await) })
                    });
                    futures_util::future::select_all(waits).await.0
                };
                let first = match limit {
                    Some(limit) => tokio::time::timeout(limit, first).await.ok(),
                    None => Some(first.await),
                };
                let Some((first_index, first_result)) = first else {
                    return Vec::new();
                };
                let mut due = vec![(first_index, first_result)];
                for (index, client) in clients.iter() {
                    let index = *index;
                    if index != first_index
                        && let Some(result) = client.next_decision().now_or_never()
                    {
                        due.push((index, result));
                    }
                }
                due.sort_by_key(|(index, _)| *index);
                due
            })
        });
        due.into_iter()
            .map(|(index, result)| match result {
                Ok(decision) => Client::decision(py, decision).map(|d| (index, Some(d))),
                Err(SdkError::Closed(_)) => {
                    self.closed
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .insert(index);
                    Ok((index, None))
                }
                Err(other) => Err(error(other)),
            })
            .collect()
    }
}
