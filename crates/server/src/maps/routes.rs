//! HTTP routes of the map service; `crate::app` merges them unchanged.
use super::{
    activate, activate_package, cancel_job, chunk, create_job, estimate, footprint, height_bounds,
    job_status, list, list_jobs, minimap, package, preview, reset,
};
use crate::AppState;
use axum::{
    Router,
    routing::{get, post},
};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/maps/estimate", post(estimate))
        .route("/maps/footprint", post(footprint))
        .route("/maps/jobs", get(list_jobs).post(create_job))
        .route("/maps/jobs/{job_id}", get(job_status))
        .route("/maps/jobs/{job_id}/cancel", post(cancel_job))
        .route("/maps/activate", post(activate))
        .route("/maps/reset", post(reset))
        .route("/maps", get(list))
        .route("/maps/{content_hash}/height-bounds", get(height_bounds))
        .route("/maps/{content_hash}/preview", get(preview))
        .route("/maps/{content_hash}/minimap", get(minimap))
        .route("/maps/{content_hash}", get(package).post(activate_package))
        .route("/maps/{content_hash}/chunks/{x}/{y}", get(chunk))
}
