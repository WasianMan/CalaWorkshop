# calaworkshop ↔ helper HTTP contract

The Calagopus extension (`dev.wasian.calaworkshop`) never touches a server volume
directly. It asks the **helper** to download Steam Workshop items, then tells Wings
to `files/pull` the result into the server. This file is the single source of truth
for the wire format between the two.

## Transport & auth

- Helper listens on `WORKSHOP_HELPER_BIND` (default `0.0.0.0:8090`).
- All endpoints **except `GET /files/...`** require `Authorization: Bearer <WORKSHOP_HELPER_TOKEN>`.
- `GET /files/...` is unauthenticated-by-header (Wings pull cannot send custom headers),
  but requires a per-job `?token=<file_token>` query param. The token is random per job
  and only returned to the authenticated extension. This is what lets Wings fetch the file.
- In AIO the helper is **not** published to the host — only reachable on the compose
  network as `http://calagopus-workshop-helper:8090`. Wings (bundled in the AIO panel
  container) reaches it over that network.

## Errors

JSON `{ "error": "message" }` with the appropriate 4xx/5xx status. `401` for bad bearer
token, `403` for bad file token, `404` for unknown job/account, `409` when a login needs
a Steam Guard code.

---

## Jobs

### `POST /download`
Start a workshop download.

Request:
```json
{
  "app_id": 550,
  "workshop_id": 123456789,
  "account": null,            // null = anonymous; else a linked account label
  "archive": false            // true = zip the item folder; false = serve the single largest file (e.g. one .vpk)
}
```

Response `202 Accepted`:
```json
{
  "id": "9f1c...uuid",
  "state": "queued",
  "file_token": "base64url-random"   // used to build the /files URL for Wings
}
```

### `GET /jobs/{id}`
Poll a job.

Response `200`:
```json
{
  "id": "9f1c...uuid",
  "state": "queued | downloading | ready | failed",
  "app_id": 550,
  "workshop_id": 123456789,
  "file_name": "123456789.vpk",   // present when state == ready
  "file_token": "base64url-random",
  "size": 1234567,                 // bytes, present when ready
  "error": null                    // human-readable string when state == failed
}
```

### `GET /files/{id}?token={file_token}`
Stream the downloaded artifact. Called by **Wings**, not the extension.
- `Content-Disposition: attachment; filename="<file_name>"` is set so Wings `use_header=true`
  names the file correctly.
- `403` if token mismatch, `404` if job unknown, `409` if job not yet `ready`.

---

## Accounts (Steam linking — Phase 5)

Sessions are cached on disk per label; passwords are not persisted long-term once a
session is established. Steam Guard is the painful part: a fresh login may require a code.

### `GET /accounts`
```json
{ "accounts": [ { "label": "wasian-main", "valid": true } ] }
```

### `POST /accounts/login`
Request:
```json
{ "label": "wasian-main", "username": "steamuser", "password": "...", "guard_code": null }
```
- `200 { "state": "ok" }` — session established/refreshed.
- `409 { "state": "needs_guard" }` — re-call with `guard_code` filled in.
- `401 { "error": "invalid credentials" }`.

### `DELETE /accounts/{label}`
Removes the cached session. `204`.

---

## SteamCMD facts baked into the helper (verified, not guessed)

- Anonymous works only for apps on Valve's allow-list; **L4D2 (550) generally requires an
  owning account** → expect to use a linked account for L4D2.
- There is **no passwordless download token**. Auth = login once (+ Guard code), session
  cached in the steam home dir; reused until it expires.
- Workshop content lands at `<steam_workdir>/steamapps/workshop/content/{app_id}/{workshop_id}/`.
- Command shape:
  `steamcmd +force_install_dir <dir> +login <anonymous|user pass [code]> +workshop_download_item <app_id> <workshop_id> +quit`
