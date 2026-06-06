//! Calagopus Workshop — Steam Workshop downloader extension.
//!
//! AI/dev note: extension model docs live at https://calagopus.com/ai-doc/extensions.md
//! The wire contract with the helper sidecar is in this repo's `CONTRACT.md`.

use indexmap::IndexMap;
use shared::{
    State,
    extensions::{Extension, ExtensionPermissionsBuilder, ExtensionRouteBuilder},
    permissions::PermissionGroup,
};
use std::sync::Arc;

mod helper;
mod routes;
mod settings;

#[derive(Default)]
pub struct ExtensionStruct;

#[async_trait::async_trait]
impl Extension for ExtensionStruct {
    async fn initialize_router(
        &mut self,
        state: State,
        builder: ExtensionRouteBuilder,
    ) -> ExtensionRouteBuilder {
        builder
            // Per-server: paste/search → install → manage. /api/client/servers/{server}/calaworkshop
            .add_client_server_api_router(|router| {
                router.nest("/calaworkshop", routes::server::router(&state))
            })
            // User-scoped Steam account linking. /api/client/calaworkshop
            .add_client_api_router(|router| {
                router.nest("/calaworkshop", routes::user::router(&state))
            })
            // Admin config. /api/admin/extensions/dev.wasian.calaworkshop
            .add_admin_api_router(|router| {
                router.nest(
                    "/extensions/dev.wasian.calaworkshop",
                    routes::admin::router(&state),
                )
            })
    }

    async fn initialize_permissions(
        &mut self,
        _state: State,
        builder: ExtensionPermissionsBuilder,
    ) -> ExtensionPermissionsBuilder {
        builder
            .add_server_permission_group(
                "workshop",
                PermissionGroup {
                    description: "Steam Workshop content for this server.",
                    permissions: IndexMap::from([
                        ("read", "View the Workshop tab and installed content."),
                        ("install", "Download and install Workshop items."),
                        ("remove", "Remove installed Workshop content."),
                    ]),
                },
            )
            .add_user_permission_group(
                "calaworkshop",
                PermissionGroup {
                    description: "Calagopus Workshop account settings.",
                    permissions: IndexMap::from([(
                        "link-steam",
                        "Link and manage Steam accounts used for downloads.",
                    )]),
                },
            )
            .add_admin_permission_group(
                "calaworkshop",
                PermissionGroup {
                    description: "Calagopus Workshop administration.",
                    permissions: IndexMap::from([(
                        "configure",
                        "Configure the helper connection, Steam API key, and game presets.",
                    )]),
                },
            )
    }

    async fn settings_deserializer(
        &self,
        _state: State,
    ) -> shared::extensions::settings::ExtensionSettingsDeserializer {
        Arc::new(settings::ExtensionSettingsDataDeserializer)
    }
}
