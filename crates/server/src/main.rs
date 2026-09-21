use aoe_server::{AppState, Config, app};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let config = Config::from_env()?;
    let build = std::env::var("AOE_BUILD_SHA")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| option_env!("AOE_BUILD_SHA").unwrap_or("local").to_owned());
    let state = AppState::new(&config, build)?;
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address = %config.bind, scenario = config.scenario.name, "AoeWorld listening");
    tokio::spawn(state.clone().run_ticks());
    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(error) => {
                    tracing::error!(%error, "cannot listen for shutdown signals");
                    return;
                }
            };
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    tracing::error!(%error, "cannot listen for interrupt signal");
                }
            }
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "cannot listen for interrupt signal");
    }
}
