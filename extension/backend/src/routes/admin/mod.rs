use shared::State;
use utoipa_axum::router::OpenApiRouter;

mod settings;

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .nest("/settings", settings::router(state))
        .nest("/diagnostics", settings::diagnostics_router(state))
        .with_state(state.clone())
}
