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
  "archive": false            // true = zip the item folder; false = select install artifacts (VPK + matching image)
}
```

Response `202 Accepted`:
```json
{
  "id": "9f1c...uuid",
  "state": "queued",
  "file_token": "base64url-random"   // helper-internal; the extension does not expose this in its start response
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
  "file_name": "workshop_123456789.zip",   // present when state == ready
  "files": ["123456789.vpk", "123456789.jpg"],
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

For normal non-archive installs, the helper can serve a generated zip containing
only the selected install artifacts. The `files` array tells the extension which
filenames should exist after Wings decompresses the transfer zip.

### `GET /health`
Authenticated reachability check.

### `GET /diagnostics/steamcmd`
Authenticated SteamCMD anonymous-login connectivity check. Returns
`{ "ok": true, "message": "..." }` or a non-2xx response with a parsed error.

---

## Accounts (Steam linking)

Sessions are cached on disk per label; passwords are never persisted. Steam Guard is
the painful part: a fresh login may require a code.

**`label` here is opaque.** Ownership and the friendly name a user types live in the
extension's `dev_wasian_calaworkshop_steam_links` table; the extension maps each
user's friendly label to a random per-link `helper_label` and only ever sends that
opaque value to the helper. The helper treats the label purely as a session
directory name and enforces no ownership itself — keeping users isolated is the
extension's job, and it is why the helper must stay on the internal network.

### `GET /accounts`
```json
{ "accounts": [ { "label": "f3a9...opaque", "valid": true } ] }
```

### `POST /accounts/login`
Request:
```json
{ "label": "f3a9...opaque", "username": "steamuser", "password": "...", "guard_code": null }
```
- `200 { "state": "ok" }` — session established/refreshed.
- `409 { "state": "needs_guard" }` — re-call with `guard_code` filled in.
- `401 { "error": "invalid credentials" }`.

Login (and the connectivity check) run steamcmd with `+@ShutdownOnFailedCommand 1
+@NoPromptForPassword 1` and under a hard timeout, so a blocked socket or a Guard
prompt steamcmd can't satisfy non-interactively fails fast instead of hanging.

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
  `steamcmd +@ShutdownOnFailedCommand 1 +@NoPromptForPassword 1 +force_install_dir <dir> +login <anonymous|user pass [code]> +workshop_download_item <app_id> <workshop_id> +quit`
- The Steam Guard code is the optional 3rd positional arg to `+login`. After a
  successful login the session is cached in the work dir and later `+login <user>`
  calls reuse it without a code.
