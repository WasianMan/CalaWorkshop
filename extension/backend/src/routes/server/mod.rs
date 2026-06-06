use shared::State;
use utoipa_axum::router::OpenApiRouter;

mod config;
mod downloads;
mod installed;

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .nest("/config", config::router(state))
        .nest("/downloads", downloads::router(state))
        .nest("/installed", installed::router(state))
        .with_state(state.clone())
}
