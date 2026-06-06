//! calaworkshop-helper
//!
//! A standalone, self-contained HTTP microservice that drives `steamcmd` to
//! download Steam Workshop items on behalf of the Calagopus `calaworkshop`
//! extension. It implements the wire contract in `../CONTRACT.md` and has no
//! Calagopus dependencies of its own.

mod config;
mod routes;
mod state;
mod steamcmd;

use std::net::SocketAddr;

use anyhow::{Context, Result};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    // Refuses to start (returns Err -> non-zero exit) if the token is missing.
    let config = Config::from_env().context("loading configuration")?;

    // Ensure the data dirs exist up front so first requests don't race on them.
    tokio::fs::create_dir_all(config.data_dir.join("jobs"))
        .await
        .ok();
    tokio::fs::create_dir_all(config.data_dir.join("steam"))
        .await
        .ok();

    let bind = config.bind.clone();
    let state = AppState::new(config);
    let app = routes::router(state);

    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("invalid WORKSHOP_HELPER_BIND '{bind}'"))?;

    tracing::info!(%addr, "calaworkshop-helper listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    axum::serve(listener, app).await.context("server error")?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,calaworkshop_helper=debug"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
