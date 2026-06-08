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
1. User selects a configured game, then pastes a Workshop URL/ID, chooses a
   search result, or previews a collection in the Workshop tab and clicks install.
2. Frontend → extension:  POST /api/client/servers/{server}/calaworkshop/downloads
3. Extension → helper:    POST /download {app_id, workshop_id, account?, archive,
                          title_slug?, install_rule}
                          (the extension resolves install_rule + generated files
                           + auth + post_install from the game preset for this app_id)
                          helper runs SteamCMD, returns a job id + file token
4. Frontend polls:        GET  …/downloads/{job}        (extension merges persisted DB state with helper /jobs)
5. When the helper job is "ready":
   Frontend → extension:  POST …/downloads/{job}/install {install_path}
   Extension → Wings:     post_servers_server_files_pull(server, {
                            root: install_path,
                            url:  http://helper:8090/files/{job}?token=…,
                            file_name, use_header:false, foreground:true })
                          (+ decompress + delete the transfer zip; if the preset's
                           persisted post_install == "extract", also unpack any
                           archive among the installed files)
```

Search and collection expansion are extension-backend concerns. The frontend calls
`GET …/calaworkshop/search` for `IPublishedFileService/QueryFiles` results and
`POST …/calaworkshop/collections/preview` for
`ISteamRemoteStorage/GetCollectionDetails`; collection install then creates normal
download jobs for each child item. Search supports empty-query explore results,
sort modes, item/collection mode, and tag filters discovered from returned Steam
items. The helper still only knows how to download a single Workshop item with
SteamCMD.

### Game presets & install rules

Per-game behavior is **data**, not code. Each preset in the extension settings
carries `app_id`, `name`, `install_path`, an `auth` requirement, a `post_install`
action, `match` rules, optional `extract_files`, optional `generated_files`, and
`scan` rules. At download time the extension resolves the preset for the requested
`app_id` and sends the install rule to the helper, which selects, renames,
extracts, and generates files accordingly (see [CONTRACT.md](../CONTRACT.md)).
The L4D2 `<workshop_id>.vpk` rename is now just the default preset's rule, so new
games need no helper code. An app id with no preset falls back to "mirror every
downloaded file". `post_install` is persisted on the download row so the install
step is driven by server-side state, not a client flag.

Garry's Mod is configured the same way: the preset root is `garrysmod`, the
downloaded `.gma`/legacy payload is extracted to
`addons/{title_slug}_{workshop_id}/`, and a generated
`lua/autorun/server/cala_workshop_{workshop_id}.lua` file calls
`resource.AddWorkshop("<workshop_id>")` so clients fetch the Workshop content.
The helper extracts GMAD payloads itself rather than trying to run `gmad` inside
the game server container, because placement goes through Wings `files/pull` and
must work on remote nodes. Generated files can run game server code, so preset
editing is intentionally gated behind the admin `calaworkshop.configure`
permission.

See [`GAME_PRESETS.md`](./GAME_PRESETS.md) for authoring guidance,
[`games.example.json`](./games.example.json) for tested built-in presets, and
[`advanced-rule.example.json`](./advanced-rule.example.json) for the exact
Advanced-box JSON shape.

### App-id auto-detection

The Workshop tab preselects a game by best-effort detection. Calagopus servers
don't expose a Steam app id as a first-class field, so `GET …/config` reads the
server's egg variables (`ServerVariable`) and `startup`, and matches candidate
numbers against configured presets, scored **high** (a variable named like
`*APP_ID*`/`SRCDS_APPID`), **medium** (a number next to `app_update` /
`workshop_download_item` / `+app_id` in the startup), or **low** (any numeric
variable that matches a preset). The highest non-empty tier wins; an ambiguous
tier returns nothing rather than guessing. The user can always override.

### Why Wings `files/pull` instead of a bind-mounted sidecar

The naive approach — give the helper a bind mount of `/var/lib/pterodactyl/volumes`
and copy files in — only works when the helper shares a filesystem with the node,
i.e. **AIO only**. Calagopus's Wings client already exposes
`post_servers_server_files_pull`, which makes the node fetch a URL straight into the
server volume. By handing Wings the helper's `/files/<job>` URL, **Wings does the
placement**, so the same code path works on AIO and on remote nodes with no extra
work. The helper never touches a volume.

(Verified against the panel's own `files/pull` route and the `wings-api` client.)

**Wings blocks private-IP pulls by default.** Its `api.remote_download_blocked_cidrs`
SSRF guard blocks RFC1918 ranges, and the helper lives on a private compose IP, so
the operator must allow the helper's range for installs to work — see
[DEPLOY.md](./DEPLOY.md#3-allow-wings-to-pull-from-the-helper).

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
  is optional for direct installs but required for Workshop search. It is used only
  for names, previews, search, and collection metadata; SteamCMD handles downloads.
  The two secrets are encrypted at rest via
  `database.encrypt`/`decrypt` (+ base32). Presets are stored as a single serde blob.
- **Steam API cache** lives in `dev_wasian_calaworkshop_steam_cache`. Search,
  collection, and direct-item metadata responses are cached as JSON with short TTLs
  so multiple panel users do not repeatedly hit Steam for the same browse data. The
  cache stores preview URLs, not image blobs; browsers load images from Steam/CDN.
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

Installed-content scanning is also preset-driven. L4D2 scans
`left4dead2/addons` and `left4dead2/addons/workshop`; GMod scans
`garrysmod/addons` for addon folders. Managed installs are de-duplicated against
scan results even when the preset root differs from the scanned subdirectory
(for example `garrysmod` plus `addons/foo_123/`).

## Helper internals

- In-memory job registry; artifacts written under `WORKSHOP_DATA_DIR/jobs/<id>/`.
- **Install-rule evaluation:** for non-archive downloads the helper applies the
  preset's `match`/`extract_files` rules to the SteamCMD content folder and writes
  a transfer zip using rendered install destinations plus any generated files.
  The default L4D2 preset matches the common `*_legacy.bin` raw VPK and paired
  preview image, renaming them to `<workshop_id>.vpk` and `<workshop_id>.<ext>`.
  The default GMod preset extracts `*_legacy.bin`/`*.gma` to
  `addons/{title_slug}_{workshop_id}/` and adds the per-item client-download Lua
  file. GMAD extraction parses the archive index and streams file ranges from the
  source payload into the zip, with path traversal and size caps.
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
