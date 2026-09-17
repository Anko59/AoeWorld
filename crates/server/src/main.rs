use aoe_server::{AppState, Config, app};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let config = Config::from_env()?;
    let state = AppState::new(&config, option_env!("AOE_BUILD_SHA").unwrap_or("local"));
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address = %config.bind, scenario = config.scenario.name, "AoeWorld Harness Lab listening");
    tokio::spawn(state.clone().run_ticks());
    axum::serve(listener, app(state)).await?;
    Ok(())
}
