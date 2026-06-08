# Deploy Guide

How to run calaworkshop on a live Calagopus AIO install. The examples assume
Coolify or plain `docker compose`, and use the public release artifacts:

- Helper image: `ghcr.io/wasianman/calaworkshop-helper:<version>` or `:latest`
- Extension archive: `CalaWorkshop-v<version>.c7s.zip` from the GitHub Release

## Prerequisites

- A working Calagopus **AIO** deployment.
- Ability to run the panel **heavy** image (`ghcr.io/calagopus/panel:heavy-aio`).
  It recompiles the panel on startup to bake in extensions, so expect about
  1-2 minutes of panel downtime and a temporary memory spike during deploy.
- Your existing `APP_ENCRYPTION_KEY`. Keep it byte-for-byte; changing it makes
  encrypted settings/secrets unreadable.

## 1. Generate a Helper Token

Generate one secret and use the same value in Docker/Coolify and in the extension
admin settings:

```bash
openssl rand -hex 32
# WORKSHOP_HELPER_TOKEN=<that value>
```

## 2. Edit Compose

Use [`../compose.aio.example.yml`](../compose.aio.example.yml) as the reference.
The important changes are:

1. Change the panel image from `ghcr.io/calagopus/panel:aio` to
   `ghcr.io/calagopus/panel:heavy-aio`.
2. Add the four heavy-image build mounts to the `web` service:

   ```yaml
   volumes:
     - '/data/calagopus/build/binaries:/app/binaries'
     - '/data/calagopus/build/translations:/app/translations'
     - '/data/calagopus/build/extensions:/app/extensions'
     - '/data/calagopus/build/extension-migrations:/app/repo/database/extension-migrations'
   ```

3. Add the helper service:

   ```yaml
   calagopus-workshop-helper:
     image: 'ghcr.io/wasianman/calaworkshop-helper:latest'
     container_name: calagopus-workshop-helper
     restart: unless-stopped
     # SteamCMD currently needs this on newer Docker releases. See the
     # SteamCMD/seccomp section below for the narrower alternative.
     security_opt:
       - seccomp=unconfined
     environment:
       - 'WORKSHOP_HELPER_TOKEN=${WORKSHOP_HELPER_TOKEN}'
       - WORKSHOP_HELPER_BIND=0.0.0.0:8090
       - WORKSHOP_DATA_DIR=/data
     volumes:
       - '/data/calagopus/workshop-helper:/data'
   ```

The helper is not published to the host. The panel/Wings side reaches it over the
compose network at `http://calagopus-workshop-helper:8090`, which is also the
default helper URL in the extension settings.

### Coolify Notes

- Keep the helper in the same Docker Compose resource as the panel so service-name
  DNS (`calagopus-workshop-helper`) resolves.
- Add `WORKSHOP_HELPER_TOKEN` to the resource environment.
- Make `/data/calagopus/workshop-helper` persistent. It holds cached SteamCMD
  sessions and temporary download artifacts.
- Keep the four `/app/...` build mounts persistent so the compiled panel and
  installed extension survive redeploys.

## 3. Allow Wings to Pull From the Helper

Installs work by giving Wings a helper `/files/<job>` URL, then Wings pulls that
file into the server volume. Wings has an SSRF guard
(`api.remote_download_blocked_cidrs`) that blocks private IP ranges by default.
Because the helper lives on the private compose/Coolify network, downloads can
succeed while installs fail with a `417` / "Network unreachable" until this is
allowed.

Before deploying, decide which private range your helper network uses. Existing
installs can check it with:

```bash
docker inspect calagopus-workshop-helper --format '{{range .NetworkSettings.Networks}}{{.IPAddress}} {{.NetworkID}}{{end}}'
docker network inspect <network-id-or-name> --format '{{json .IPAM.Config}}'
```

Then edit Wings config. In this AIO setup it is usually mounted at
`/data/calagopus/wings-config.yml` and injected into the panel as
`AIO_BASE_WINGS_CONFIGURATION=/wings-config.yml`.

Example for a helper on `10.x`:

```yaml
api:
  remote_download_blocked_cidrs:
    - 127.0.0.0/8
    # - 10.0.0.0/8        # removed so Wings can pull from the helper
    - 172.16.0.0/12
    - 192.168.0.0/16
    - 169.254.0.0/16
    - ::1
    - fe80::/10
    - fc00::/7
```

If Docker gives the helper a `172.16-31.x` address, remove `172.16.0.0/12`
instead. If it gives the helper a `192.168.x` address, remove
`192.168.0.0/16` instead. Prefer the narrowest subnet Wings accepts for your
helper network.

After changing this file, the panel/Wings process must restart or redeploy to load
it. If the AIO image rewrites the file on deploy, bake the change into whatever
base config `AIO_BASE_WINGS_CONFIGURATION` points at.

## 4. Install the Extension Archive

Download `CalaWorkshop-v<version>.c7s.zip` from the GitHub Release. You can either
upload it from the Calagopus **Extensions** page, or place it in the host path
mounted to `/app/extensions`:

```bash
mkdir -p /data/calagopus/build/extensions
cp CalaWorkshop-v0.2.6.c7s.zip /data/calagopus/build/extensions/
```

If you upload through the Extensions page and the panel does not rebuild
automatically, click **Rebuild Extensions** on that page. If you install by
copying into `/app/extensions`, restart/redeploy `web` so the heavy panel compiles
the extension and runs migrations.

## 5. Rebuild Extensions or Restart

```bash
docker compose pull calagopus-workshop-helper
docker compose up -d
docker compose logs -f web
```

In Coolify, redeploy the compose resource instead. Watch the `web` logs: the heavy
panel should detect the `.c7s.zip`, compile the extension, run migrations, and
start normally. If you uploaded the archive through the Extensions page, the
**Rebuild Extensions** button is usually enough; restart `web` only if the rebuild
does not pick up the new archive or you copied the zip into the mounted directory.

## 6. Configure Calaworkshop

1. Admin -> **Extensions -> CalaWorkshop**:
   - **Helper URL**: `http://calagopus-workshop-helper:8090`
   - **Helper token**: the same `WORKSHOP_HELPER_TOKEN`
   - **Steam Web API key**: optional for direct installs, required for Workshop
     search/explore, and used for names/previews/collection metadata. SteamCMD
     still handles downloads.
   - **Game presets**: Left 4 Dead 2 should point at `left4dead2/addons`;
     Garry's Mod should point at `garrysmod` and use the default GMAD extraction
     rule.
2. Run the admin diagnostics. Helper and SteamCMD should both report healthy.
3. Grant server permissions as needed:
   - `workshop.read`
   - `workshop.install`
   - `workshop.remove`
4. Grant `calaworkshop.link-steam` to users who should link Steam accounts.
5. For L4D2, link a Steam account that owns the game on the **Steam Link** account
   page. Anonymous downloads generally do not work for app `550`.

## Updating

- If the release includes helper changes, pull the new public helper image and
  redeploy/restart the helper.
- Upload the new `CalaWorkshop-v<version>.c7s.zip` in the Extensions page, or
  replace it in `/app/extensions`.
- Click **Rebuild Extensions** if the panel does not rebuild automatically, or
  restart `web` if you updated the mounted file directly.

When a release notes helper changes, update the helper image and `.c7s.zip`
together. The helper/extension HTTP contract can change between releases. For
documentation-only or extension-only releases, the helper can stay on the existing
image.

## Reverting

Change `ghcr.io/calagopus/panel:heavy-aio` back to `ghcr.io/calagopus/panel:aio`
and redeploy. The stock image ignores the `/app/...` build mounts. You can leave
the helper service and persistent data in place for a later reinstall.

## SteamCMD Connectivity on Newer Docker

If admin diagnostics show the helper reachable but SteamCMD fails with
`CreateBoundSocket: failed to create socket ... (38)`, `No Connection`, or
`steamcmd timed out after 90s`, this is usually not a missing network route.
Docker 29.4.2 tightened its default seccomp profile for CVE-2026-31431 and can
block socket paths SteamCMD still uses.

The example compose uses this blunt but scoped workaround on the helper only:

```yaml
calagopus-workshop-helper:
  security_opt:
    - seccomp=unconfined
```

A narrower production option is a custom seccomp profile based on Docker's default
that re-allows `AF_ALG` plus 32-bit `socketcall`:

```yaml
calagopus-workshop-helper:
  security_opt:
    - seccomp=/etc/docker/seccomp/default-plus-afalg.json
```

Do not set this on the whole stack, and do not switch to host networking. After
changing `security_opt`, recreate the helper and rerun diagnostics before trying
Steam linking or downloads.

The helper image is intentionally minimal and may not include `curl`, `wget`,
`ping`, or DNS tools. If you need general network testing, run a temporary
diagnostics container on the same Docker network. The decisive check for Workshop
downloads is still the admin SteamCMD diagnostic, because plain HTTP/DNS can work
while SteamCMD's socket path is blocked.

## Troubleshooting

| Symptom | Likely cause / fix |
| --- | --- |
| Panel log shows a Rust compile error from `dev_wasian_calaworkshop` | Extension/panel API mismatch. Open an issue with the log. |
| Helper exits immediately | Missing `WORKSHOP_HELPER_TOKEN`; the helper refuses to start without it. |
| Workshop tab says "helper is not configured" | Helper URL/token not set in admin settings, or token mismatch with the helper env var. |
| Admin diagnostics: helper OK, SteamCMD failed | Check the `security_opt` / seccomp section above. |
| L4D2 downloads fail anonymously | Expected. Link a Steam account that owns L4D2 and select it for the download. |
| Install fails with Wings `417` / "Network unreachable" | Wings is still blocking the helper's private IP. Revisit `remote_download_blocked_cidrs`, restart/redeploy, and confirm the helper subnet. |
| Wings cannot fetch the helper URL | Panel/Wings and helper are not on the same compose network, or the helper URL is not the service DNS name. |
