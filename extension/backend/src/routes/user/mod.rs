use shared::State;
use utoipa_axum::router::OpenApiRouter;

mod steam;

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .nest("/steam/accounts", steam::router(state))
        .nest("/steam/login-sessions", steam::sessions::router(state))
        .with_state(state.clone())
}
