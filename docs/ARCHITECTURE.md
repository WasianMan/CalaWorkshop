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
4. Frontend polls:        GET  …/downloads/{job}        (extension proxies helper /jobs)
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
  game presets) live in the panel's extension settings store. The two secrets are
  encrypted at rest via `database.encrypt`/`decrypt` (+ base32). Presets are stored as
  a single serde blob.
- **Per-user data** (which panel user owns which Steam label) is *not* expressible in
  global settings, so a migration table (`dev_wasian_calaworkshop_steam_links`) is
  scaffolded for the planned per-user ownership scoping. v1 treats Steam linking as a
  thin proxy suitable for a single-admin panel.
- The panel's `state.storage` is **not** used — that's panel-level storage (avatars,
  reports), not server volumes. Server files always go through Wings.

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

## Multi-node (future)

Because placement goes through Wings, remote nodes work — but each node's Wings must
be able to reach the helper's URL. For multi-node deployments that means exposing the
helper at a network location all nodes can hit (or running a helper per node). This is
documented as a roadmap item rather than wired up in v1.
