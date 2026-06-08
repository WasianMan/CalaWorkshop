use shared::State;
use utoipa_axum::router::OpenApiRouter;

mod collections;
mod config;
mod downloads;
mod installed;
mod search;

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .nest("/config", config::router(state))
        .nest("/collections", collections::router(state))
        .nest("/downloads", downloads::router(state))
        .nest("/installed", installed::router(state))
        .nest("/search", search::router(state))
        .with_state(state.clone())
}
