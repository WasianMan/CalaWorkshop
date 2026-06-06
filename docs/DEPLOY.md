# Deploy guide

How to run calaworkshop on a live Calagopus AIO install. Written with **Coolify**
in mind (where you can't rely on a local `build:` context), but the steps apply to
plain `docker compose` too.

## Prerequisites

- A working Calagopus **AIO** deployment.
- Ability to run the **heavy** image (`:heavy-aio`). It ships the Rust + Node
  toolchains and **recompiles the panel on startup** to bake in extensions, so:
  - expect **~1–2 min of panel downtime** on the deploy that installs/updates the
    extension, and
  - more RAM than `:aio` (the build spikes memory). Do it in a quiet window.
- Your existing `APP_ENCRYPTION_KEY` — **keep it byte-for-byte**. Changing it makes
  every encrypted secret in your DB unrecoverable.

## 1. Publish the helper image (one time + on each release)

CI builds and pushes the helper image to GHCR when you push a tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The `release` workflow publishes `ghcr.io/<owner>/calaworkshop-helper` with tags
`0.1.0`, `0.1`, and `latest`, and creates a GitHub Release with the
`dev_wasian_calaworkshop.c7s.zip` attached.

**Make the package pullable by Coolify.** Easiest: open the package on GitHub
(Profile → Packages → calaworkshop-helper → Package settings) and set visibility to
**Public**. (Alternatively keep it private and add GHCR registry credentials in
Coolify → Keys & Tokens → Registries.)

## 2. Edit your compose

Use [`../compose.aio.example.yml`](../compose.aio.example.yml) as the reference. The
three changes to your existing stack:

1. **Panel image:** `ghcr.io/calagopus/panel:aio` → `:heavy-aio`.
2. **Add four build mounts** to the `web` service `volumes:` (host paths are created
   on start):
   ```yaml
   - '/data/calagopus/build/binaries:/app/binaries'
   - '/data/calagopus/build/translations:/app/translations'
   - '/data/calagopus/build/extensions:/app/extensions'
   - '/data/calagopus/build/extension-migrations:/app/repo/database/extension-migrations'
   ```
3. **Add the helper service**, pulling the published image:
   ```yaml
   calagopus-workshop-helper:
     image: 'ghcr.io/<owner>/calaworkshop-helper:latest'
     container_name: calagopus-workshop-helper
     restart: unless-stopped
     environment:
       - 'WORKSHOP_HELPER_TOKEN=${WORKSHOP_HELPER_TOKEN}'
       - WORKSHOP_HELPER_BIND=0.0.0.0:8090
       - WORKSHOP_DATA_DIR=/data
     volumes:
       - '/data/calagopus/workshop-helper:/data'
   ```

The helper is **not** published to the host — it's only reachable on the compose
network as `http://calagopus-workshop-helper:8090`, which is the default helper URL
in the extension settings. Wings (bundled in the AIO panel container) pulls finished
downloads from it over that network.

### Coolify notes

- If your panel is a **Docker Compose** resource in Coolify, paste the edited compose
  into the same resource so all services share one project network (service-name DNS
  like `calagopus-workshop-helper` then resolves).
- Add `WORKSHOP_HELPER_TOKEN` as an environment variable on the resource (Coolify
  injects it into the compose just like `.env`).
- Ensure the helper's `/data` maps to a **persistent** path/volume — it holds cached
  SteamCMD sessions and downloads. `/data/calagopus/workshop-helper` works if that's
  a persistent host path; otherwise use a named volume.
- The four `/app/...` build paths must also be persistent (so the compiled panel and
  the installed extension survive redeploys).

## 3. Set the helper token

Generate once and put the **same** value in your env and (later) the admin settings:

```bash
openssl rand -hex 32
# WORKSHOP_HELPER_TOKEN=<that value>
```

## 4. Install the extension archive

Download `dev_wasian_calaworkshop.c7s.zip` from the GitHub Release (or build it
locally) and place it in the host path mapped to `/app/extensions`:

```bash
mkdir -p /data/calagopus/build/extensions
cp dev_wasian_calaworkshop.c7s.zip /data/calagopus/build/extensions/
```

## 5. Deploy

```bash
docker compose up -d            # or redeploy the resource in Coolify
docker compose logs -f web      # watch the panel recompile (~1–2 min)
```

On startup the heavy panel detects the `.c7s.zip`, compiles it in, and loads it. If
you add/replace the zip while the panel is already running, **restart `web`** to pick
it up.

## 6. Configure

1. Admin → **Extensions → Calagopus Workshop**:
   - **Helper URL**: `http://calagopus-workshop-helper:8090` (default).
   - **Helper token**: the same value as `WORKSHOP_HELPER_TOKEN`.
   - **Steam Web API key** (optional, for future search/metadata).
   - **Game presets** — Left 4 Dead 2 → `left4dead2/addons/workshop` is seeded.
2. Grant the `workshop.read` / `workshop.install` / `workshop.remove` **server**
   permissions to yourself/subusers, and `calaworkshop.link-steam` (user) for linking.
3. On a server, open the **Workshop** tab, paste a Workshop URL/ID, and install.
4. For Left 4 Dead 2, link a Steam account on the **Steam Link** account page first
   (anonymous downloads won't work for app 550).

## Updating

- **Helper:** push a new tag → CI publishes a new image → redeploy (or
  `docker compose pull calagopus-workshop-helper && docker compose up -d`).
- **Extension:** drop the new `.c7s.zip` into `/app/extensions` (replacing the old)
  and restart `web`. The panel recompiles with the new version; migrations run
  automatically.

## Reverting

Change `:heavy-aio` back to `:aio` and redeploy. The stock image ignores the
`/app/...` build mounts and starts cleanly on your existing data. You can leave the
helper service and mounts in place for when you switch back.

## Troubleshooting

| Symptom | Likely cause / fix |
| --- | --- |
| Panel log shows a Rust compile error from `dev_wasian_calaworkshop` | An extension/panel API mismatch. Open an issue with the log; it's usually a small fix. |
| Helper container restarts / exits immediately | Missing `WORKSHOP_HELPER_TOKEN` (it refuses to start without one). Check the env var. |
| Workshop tab says "helper is not configured" | Helper URL/token not set in admin settings, or token mismatch with the env var. |
| L4D2 downloads fail with anonymous | Expected — link a Steam account that owns L4D2 and select it when installing. |
| Wings "pull" fails to fetch the file | Helper not reachable from the panel/Wings network. Confirm both are on the same compose network and the URL is the service name. |
