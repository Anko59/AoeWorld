use super::*;

#[derive(Serialize)]
pub(crate) struct CreationEstimate {
    #[serde(flatten)]
    physical: MapEstimate,
    preparation: map_jobs::PreparationPlan,
}

pub(crate) async fn estimate(
    State(state): State<AppState>,
    Json(input): Json<map_jobs::CreationRequest>,
) -> Result<Json<CreationEstimate>, (StatusCode, String)> {
    let physical = input
        .request
        .estimate()
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let preparation = map_jobs::PreparationPlan::resolve(input, state.map_worker.is_some())
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    Ok(Json(CreationEstimate {
        physical,
        preparation,
    }))
}

pub(crate) async fn create_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<map_jobs::CreationRequest>,
) -> Result<(StatusCode, Json<map_jobs::Job>), (StatusCode, String)> {
    require_controller(&state, &headers).await?;
    input
        .request
        .estimate()
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let key = headers
        .get("Idempotency-Key")
        .map(|value| value.to_str().map(str::to_owned))
        .transpose()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid submission key".to_owned()))?;
    let job = map_jobs::start(&state, input, key).await.map_err(|error| {
        let code = match error {
            map_jobs::StartError::Invalid(_) => StatusCode::BAD_REQUEST,
            map_jobs::StartError::Queue(_) => StatusCode::TOO_MANY_REQUESTS,
            map_jobs::StartError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
            map_jobs::StartError::Conflict(_) => StatusCode::CONFLICT,
        };
        (code, error.to_string())
    })?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

pub(crate) async fn job_status(
    Path(job_id): Path<u64>,
    State(state): State<AppState>,
) -> Result<Json<map_jobs::Job>, StatusCode> {
    map_jobs::status(&state, job_id)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub(crate) async fn list_jobs(State(state): State<AppState>) -> Json<Vec<map_jobs::Job>> {
    Json(map_jobs::list(&state).await)
}

pub(crate) async fn cancel_job(
    Path(job_id): Path<u64>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<map_jobs::Job>, (StatusCode, String)> {
    require_controller(&state, &headers).await?;
    map_jobs::cancel(&state, job_id)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "unknown map creation job".to_owned()))
}
