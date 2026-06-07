# Architecture

## Components

| Component | Where it runs | Responsibility |
| --- | --- | --- |
| **Extension backend** (Rust) | compiled into the Calagopus panel | Authenticated routes, permissions, settings, talks to the helper, drives Wings |
| **Extension frontend** (React) | bundled into the panel UI | Workshop server tab, Steam-link account page, admin config card |
| **Helper** (Rust) | separate container | Runs SteamCMD, caches sessions, serves finished downloads over HTTP |

The two halves of the extension ship together in one `.c7s.zip`
(`Metadata.toml` + `backend/` + `frontend/` + `migrations/`). The helper is an
independent image published to GHCR.

## Install / download flow

```
1. User pastes a Workshop URL/ID in the Workshop tab and clicks install.
2. Frontend → extension:  POST /api/client/servers/{server}/calaworkshop/downloads
3. Extension → helper:    POST /download {app_id, workshop_id, account?, archive}
                          helper runs SteamCMD, returns a job id + file token
4. Frontend polls:        GET  …/downloads/{job}        (extension merges persisted DB state with helper /jobs)
5. When the helper job is "ready":
   Frontend → extension:  POST …/downloads/{job}/install {install_path, archive}
   Extension → Wings:     post_servers_server_files_pull(server, {
                            root: install_path,
                            url:  http://helper:8090/files/{job}?token=…,
                            file_name, use_header:false, foreground:true })
                          (+ decompress + delete the zip if archive)
```

### Why Wings `files/pull` instead of a bind-mounted sidecar

The naive approach — give the helper a bind mount of `/var/lib/pterodactyl/volumes`
and copy files in — only works when the helper shares a filesystem with the node,
i.e. **AIO only**. Calagopus's Wings client already exposes
`post_servers_server_files_pull`, which makes the node fetch a URL straight into the
server volume. By handing Wings the helper's `/files/<job>` URL, **Wings does the
placement**, so the same code path works on AIO and on remote nodes with no extra
work. The helper never touches a volume.

(Verified against the panel's own `files/pull` route and the `wings-api` client.)

## Permissions

Three scopes, registered in `initialize_permissions`:

- **server** `workshop.{read,install,remove}` — per-server, per-subuser control.
- **user** `calaworkshop.link-steam` — manage helper Steam accounts.
- **admin** `calaworkshop.configure` — helper/Steam-key/preset configuration.

Routes check these with `has_{server,user,admin}_permission(...)`; the frontend hides
gated UI with `ServerCan`.

## State & secrets

- **Global settings** (helper URL, helper token, Steam Web API key, default-anonymous,
  game presets) live in the panel's extension settings store. The Steam Web API key
  is optional and is used only for names, previews, and search metadata; SteamCMD
  handles downloads. The two secrets are encrypted at rest via
  `database.encrypt`/`decrypt` (+ base32). Presets are stored as a single serde blob.
- **Per-user data** (which panel user owns which Steam label) lives in the
  `dev_wasian_calaworkshop_steam_links` table. Each row ties a `user_uuid` + a
  user-facing `label` to an opaque `helper_label`. The helper keys its cached
  SteamCMD session by the opaque label, so the friendly label a user types is
  never used to address a session directly — that is what stops one user (admin
  or otherwise) from listing, downloading-as, or deleting another user's linked
  Steam account. Linking requires the `calaworkshop.link-steam` user permission.
- The panel's `state.storage` is **not** used — that's panel-level storage (avatars,
  reports), not server volumes. Server files always go through Wings.

## Persistent registry

The extension stores download jobs and installed items in PostgreSQL migration
tables. Jobs keep the panel-facing job id, helper job id, state, metadata, error
text, install path, and exact installed filenames. Installed records track the
server, app id, optional Workshop id, install path, VPK, preview image, source
(`managed`, `imported`, or scan-only `unmanaged`), and exact files used for
uninstall.

The L4D2 default preset installs to `left4dead2/addons`. The installed-content
API also scans `left4dead2/addons` and `left4dead2/addons/workshop` so existing
VPK/JPG pairs are visible before they are imported into the registry.

## Helper internals

- In-memory job registry; artifacts written under `WORKSHOP_DATA_DIR/jobs/<id>/`.
- Per-account SteamCMD working dirs under `WORKSHOP_DATA_DIR/steam/<label|anonymous>/`
  so cached sessions persist (mount `/data`).
- `GET /files/<job>?token=` is the only unauthenticated-by-header endpoint (Wings pull
  can't send custom headers); it's guarded by a per-job random token instead.
- See [`../CONTRACT.md`](../CONTRACT.md) for the exact wire format, and
  [`../helper/README.md`](../helper/README.md) for env vars and the Steam Guard caveat.

## Security notes

- Extension ↔ helper is authenticated with a shared bearer token
  (`WORKSHOP_HELPER_TOKEN`); the helper refuses to start without it.
- The helper is not published to the host in the reference compose — only reachable on
  the internal compose network.
- All mutating extension routes are permission-checked before any side effects, and
  install paths are validated (no `..`, no leading `/`) in addition to Wings' own checks.

## Concurrency: never hold the settings guard across I/O

`state.settings.get()` returns a **read guard** over a `tokio::sync::RwLock`, and the
panel reloads the settings cache (taking the *write* lock) when it expires. If a
request held that read guard across a slow network call (a hung helper, a slow Steam
API), the next cache reload would block on the write lock while holding the single
reload permit — and then *every* `settings.get()` in the panel would queue behind it,
freezing the whole panel. So every route here **snapshots the settings into an owned
value and drops the guard before any helper/Steam call**:

```rust
let ext = {
    let settings = state.settings.get().await?;
    settings.find_extension_settings::<ExtensionSettingsData>()?.clone()
}; // guard dropped here, before any network I/O
```

Belt and braces, all helper calls and the Steam metadata call also carry explicit
per-request timeouts (the panel's shared `reqwest::Client` has none), and the
installed-list route caps how many Steam metadata lookups it performs inline so a slow
Steam API can't make listing crawl.

## Multi-node (future)

Because placement goes through Wings, remote nodes work — but each node's Wings must
be able to reach the helper's URL. For multi-node deployments that means exposing the
helper at a network location all nodes can hit (or running a helper per node). This is
documented as a roadmap item rather than wired up in v1.
