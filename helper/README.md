# calaworkshop-helper

A standalone Rust HTTP microservice that drives **steamcmd** to download Steam
Workshop items for the Calagopus `dev.wasian.calaworkshop` extension. It is fully
self-contained — it has **no** Calagopus dependencies and only knows the wire
contract in [`../CONTRACT.md`](../CONTRACT.md).

The extension asks this helper to download a workshop item, polls the job, then
hands Wings a `/files/<id>?token=...` URL to pull the artifact into a server.

## Endpoints

All endpoints **except `GET /files/...`** require
`Authorization: Bearer <WORKSHOP_HELPER_TOKEN>`. `GET /files/...` is authenticated
by a per-job `?token=` query param instead (Wings cannot send custom headers).

| Method & path                 | Auth   | Purpose |
|-------------------------------|--------|---------|
| `POST /download`              | Bearer | Start a workshop download. Returns `202 {id, state:"queued", file_token}`. |
| `GET /jobs/{id}`              | Bearer | Poll job state (`queued`/`downloading`/`ready`/`failed`). |
| `GET /files/{id}?token=...`   | Token  | Stream the artifact with `Content-Disposition: attachment`. `403` token mismatch, `404` unknown job, `409` not ready. |
| `GET /health`                 | Bearer | Basic helper reachability check. |
| `GET /diagnostics/steamcmd`   | Bearer | Lightweight anonymous SteamCMD connectivity check. |
| `GET /accounts`               | Bearer | List linked accounts `{accounts:[{label, valid}]}`. |
| `POST /accounts/login`        | Bearer | Establish/refresh a cached session. `200 {state:"ok"}`, `409 {state:"needs_guard"}`, `401`. |
| `DELETE /accounts/{label}`    | Bearer | Remove a cached session. `204`. |

Errors are JSON `{ "error": "message" }` with `401`/`403`/`404`/`409`/`4xx`/`5xx`.

### `POST /download` body

```json
{ "app_id": 550, "workshop_id": 123456789, "account": null, "archive": false }
```

- `account: null` → anonymous login. A non-null label reuses that account's cached session.
- `archive: false` → select install artifacts (a `.vpk` plus same-stem `.jpg`/`.jpeg`/`.png` when present) and serve them as a small transfer zip.
- `archive: true` → zip the whole item folder into `archive.zip`.

## Configuration (env vars)

| Variable                | Default        | Notes |
|-------------------------|----------------|-------|
| `WORKSHOP_HELPER_TOKEN` | *(required)*   | Bearer token. The service refuses to start if unset/empty. |
| `WORKSHOP_HELPER_BIND`  | `0.0.0.0:8090` | Listen address. |
| `WORKSHOP_DATA_DIR`     | `/data`        | Holds `jobs/<id>/` artifacts and `steam/<label-or-anonymous>/` workdirs. |
| `STEAMCMD_BIN`          | `steamcmd`     | Path to the steamcmd executable / `.sh`. |

`RUST_LOG` controls tracing (defaults to `info,calaworkshop_helper=debug`).

## Run locally

```bash
# Linux/macOS
WORKSHOP_HELPER_TOKEN=dev WORKSHOP_DATA_DIR=./data cargo run
```

```powershell
# Windows PowerShell
$env:WORKSHOP_HELPER_TOKEN = "dev"; $env:WORKSHOP_DATA_DIR = "./data"; cargo run
```

Smoke test:

```bash
curl -X POST localhost:8090/download \
  -H "Authorization: Bearer dev" -H "Content-Type: application/json" \
  -d '{"app_id":4000,"workshop_id":121439025,"account":null,"archive":false}'
# -> {"id":"...","state":"queued","file_token":"..."}

curl -H "Authorization: Bearer dev" localhost:8090/jobs/<id>
curl -o out.bin "localhost:8090/files/<id>?token=<file_token>"
```

(You need a real `steamcmd` on `PATH`, or point `STEAMCMD_BIN` at one, for
downloads to actually succeed.)

## Docker

```bash
docker build -t calaworkshop-helper helper/
docker run --rm -e WORKSHOP_HELPER_TOKEN=dev -v workshop-data:/data \
  calaworkshop-helper
```

The runtime image is based on `steamcmd/steamcmd:latest`, which already ships
`steamcmd` on `PATH` (hence `STEAMCMD_BIN=steamcmd`). In the Calagopus AIO the
helper is **not** published to the host — it is only reachable on the compose
network as `http://calagopus-workshop-helper:8090`.

## Accounts, sessions, and the Steam Guard caveat

- **Anonymous** downloads are fully wired. They work only for apps on Valve's
  anonymous allow-list. Notably, **L4D2 (app 550) generally requires an owning
  account**, so expect to link one for it.
- There is **no passwordless download token** in steamcmd. You authenticate once
  (`POST /accounts/login`), steamcmd caches a session/sentry file in that label's
  working dir, and later downloads reuse it. The helper runs steamcmd with `HOME`
  and XDG cache/config/data directories pointed at that label directory so each
  linked account keeps an isolated, persistent SteamCMD session. After a successful
  password/Guard login, the helper immediately runs `+login <username> +quit`
  without the password to verify that cached session is reusable before marking
  the account linked.
- We persist **only the username** per label in `<data_dir>/steam/<label>/account.json`.
  The **password is never written to disk**. Account-based downloads run steamcmd
  in that workdir with `+login <username>` and rely on the cached session.
- **Steam Guard limitations:** this is a non-interactive process, so steamcmd
  cannot *prompt* for a code. A fresh, Guard-protected login will emit a
  "Steam Guard" message and exit; the helper detects that and returns
  `409 {"state":"needs_guard"}`. The caller must then re-`POST /accounts/login`
  with `guard_code` filled in (passed as the optional 3rd `+login` argument), or
  approve the mobile sign-in prompt and retry. Detection is heuristic (steamcmd
  wording varies by version). Once a session is cached it is reused until Steam
  expires it, at which point login must run again.
- `GET /accounts` reports `valid: true` when a username is stored (i.e. a login
  was performed). It does **not** re-verify session freshness — doing so would
  require invoking steamcmd on every list call.

## Layout

```
helper/
├── Cargo.toml
├── Dockerfile
├── .dockerignore
├── README.md
└── src/
    ├── main.rs       # bootstrap, tracing, axum::serve
    ├── config.rs     # env-var config + path helpers
    ├── state.rs      # AppState + in-memory job registry (Job, JobState)
    ├── steamcmd.rs   # steamcmd runner, output parsing, artifact selection, zip
    └── routes.rs     # router, bearer/token auth, all handlers
```

> **v1 note:** the helper job registry is in-memory and does not survive a restart.
> The extension persists its own job history and installed-item registry.
