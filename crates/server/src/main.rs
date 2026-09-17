use aoe_server::{AppState, Config, app};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let config = Config::from_env()?;
    let build = std::env::var("AOE_BUILD_SHA")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| option_env!("AOE_BUILD_SHA").unwrap_or("local").to_owned());
    let state = AppState::new(&config, build);
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address = %config.bind, scenario = config.scenario.name, "AoeWorld Harness Lab listening");
    tokio::spawn(state.clone().run_ticks());
    axum::serve(listener, app(state)).await?;
    Ok(())
}
