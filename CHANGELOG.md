# Changelog

All notable changes to this project are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/); versions are tag-driven.

## [0.2.6] - 2026-06-08

### Added
- Workshop search/explore UI backed by Steam `IPublishedFileService/QueryFiles`,
  including previews, sort modes, subscriptions, file sizes, vote counts, and star
  ratings. Search now loads default explore results for the selected game, supports
  item/collection mode, and exposes discovered Steam tags as filters.
- Collection preview and install flow backed by Steam
  `ISteamRemoteStorage/GetCollectionDetails`. Collection children are queued as
  ordinary download jobs so existing install/uninstall tracking still applies.
- PostgreSQL-backed Steam metadata cache for search and collection responses.

### Changed
- The Workshop page now starts with no game selected unless app-id detection is
  high/medium confidence. Low-confidence detection is shown as a hint instead of
  silently defaulting to the first preset.
- Fixed the prerelease backend build error in Steam metadata preview handling.
- Fixed Workshop search using the wrong HTTP method for Steam `QueryFiles`, and
  switched results to a compact thumbnail list for easier browsing.
- GMod item search now filters to Workshop `Addon` results so search installs
  follow the tested GMAD extraction path instead of surfacing saves/demos.
- The Steam account picker is now shared across direct, search, and collection
  installs, so account-required games like L4D2 do not require switching tabs.
- Search results now mark already-installed Workshop items and disable reinstall
  from the search list. Result count is selectable at 5, 10, 15, or 25 per page.
- Extension release archives are now named `CalaWorkshop-v<version>.c7s.zip`.

## [0.2.5] - 2026-06-08

### Added
- **Data-driven, multi-game install rules.** Game presets now carry an optional
  `auth`, a `post_install` action, and a `match` list of `{ glob, rename? }`
  rules. The helper selects + renames downloaded files by these rules instead of
  a hardcoded `app_id == 550` branch, so new games are added entirely through
  settings. Empty rules mirror every downloaded file; `rename` supports
  `{workshop_id}`/`{app_id}`/`{ext}`/`{basename}`/`{title_slug}` tokens and safe subpaths.
  Glob matching uses `globset`. An admin "Advanced (JSON)" editor authors the
  rules per preset. See [`docs/games.example.json`](./docs/games.example.json)
  for tested built-in preset examples and [`docs/advanced-rule.example.json`](./docs/advanced-rule.example.json)
  for a copy-pasteable Advanced-box example.
- **Garry's Mod default preset.** App 4000 is seeded with anonymous SteamCMD
  downloads, GMAD extraction from `*_legacy.bin`/`*.gma` into
  `addons/{title_slug}_{workshop_id}/`, and a generated per-item
  `resource.AddWorkshop` Lua file for client delivery.
- Presets can now define generated files and installed-content scan rules. The
  installed-content page no longer scans hardcoded L4D2 paths for every server.
- Presets can now define `extract_files` actions. The helper currently supports
  bounded GMAD extraction without requiring `gmad` inside the game server
  container.
- **Best-effort game auto-detection.** The Workshop tab preselects a preset by
  scoring the server's egg variables / startup command against configured app
  ids (high/medium/low confidence); the user can always override.
- New `20260609000000_post_install` migration persisting the chosen post-install
  behavior on the download row, so installs no longer trust a client-sent flag.
- Branch-test workflow for GHCR helper images and `.c7s` artifacts without
  moving release tags or `latest`.

### Changed
- The install step no longer trusts a client-supplied decompression/post-install
  flag; the transfer zip is always decompressed, and `post_install: extract`
  additionally unpacks any nested archive among the installed files.
- Existing saved L4D2 presets are hydrated with the new account requirement and
  install rules on settings load, so upgrades keep producing loadable
  `<workshop_id>.vpk` files without requiring admins to re-save settings.
- The Advanced JSON editor now uses a plain textarea with conditional rendering
  so it works reliably in the panel UI.

### Docs
- Added a game preset authoring guide and limited official examples to tested
  built-ins: Left 4 Dead 2 and Garry's Mod.

## [0.2.4]

### Added
- Steam Link now verifies a newly linked account by running a passwordless
  cached-session SteamCMD login before marking it linked.

### Fixed
- Helper SteamCMD login parsing now recognizes successful mobile-approval and
  Steam Guard-code logins when SteamCMD inserts extra post-logon compatibility
  text before the final `OK`, preventing a successful login from looping back to
  `needs_guard`.

## [0.2.3]

### Added
- Recent download rows can be removed from the Workshop page, useful for clearing
  failed transfer/install attempts without touching installed files.

### Fixed
- Helper SteamCMD account logins now isolate `HOME`/XDG cache/config/data per
  linked account label so cached sessions are reused more reliably for
  account-backed downloads.
- Steam Link no longer sends an empty Steam Guard argument, and the UI now calls
  out mobile-app sign-in approval as an alternative to typing the generated code.

## [0.2.2]

### Fixed
- **L4D2 installs now land as a loadable `<workshop_id>.vpk`.** SteamCMD delivers
  app-550 items as `<ugc-handle>_legacy.bin` (the raw VPK); the helper now renames
  the primary artifact to `<workshop_id>.vpk` (and a paired image to
  `<workshop_id>.<ext>`) so the dedicated server actually loads the addon, instead
  of dropping a stray `..._legacy.bin` the game ignores.
- **Steam Guard mobile-authenticator logins are recognized as success.** Login
  output parsing checked the "Steam Guard" notice before the success lines, so a
  phone-confirmed mobile-authenticator login was misreported as `needs_guard` and
  the session was never persisted (later account downloads failed with "no cached
  session"). Success is now checked first.
- Helper login now returns `503` with the SteamCMD connectivity error (instead of
  `401 invalid credentials`) when SteamCMD can't reach Steam, so the UI stops
  blaming the password for a connectivity/seccomp problem.

### Docs
- DEPLOY: documented the **required** Wings `remote_download_blocked_cidrs` change
  (Wings blocks pulling from the helper's private IP by default — installs fail with
  a `417`/"Network unreachable" until the helper's range is allowed).
- compose.aio.example.yml ships the helper with `security_opt: seccomp=unconfined`
  (with a pointer to a narrower custom profile) for the Docker 29.4.2 SteamCMD fix.

## [0.2.1]

### Added
- **Per-user Steam account linking.** Linked accounts are now owned by the panel
  user who created them and addressed on the helper through an opaque per-link
  label, so no user (admin included) can see, download with, or delete another
  user's linked Steam account. Linking uses the `calaworkshop.link-steam` user
  permission (no longer admin-only). New `20260608000000_steam_links` migration.

### Fixed
- **Panel no longer locks up during Steam/helper operations.** Routes were holding
  the settings read guard (a `tokio::sync::RwLock` guard) across helper/Steam
  network calls; when the settings cache reload needed the write lock, the whole
  panel could stall. Every route now snapshots settings and drops the guard before
  any network I/O.
- Added explicit per-request timeouts to all helper calls and the Steam metadata
  call (the panel's shared HTTP client has none), so a hung helper can't pin a
  request — or, via the settings guard, the panel.
- Hardened the helper's SteamCMD invocations with `+@ShutdownOnFailedCommand 1
  +@NoPromptForPassword 1` and hard timeouts, so a blocked socket (newer-Docker
  seccomp) or an unsatisfiable Steam Guard prompt fails fast instead of hanging.
- Capped inline Steam metadata lookups on the installed-content list so a slow
  Steam API can't make the list crawl.

### Docs
- Documented the settings-guard concurrency rule, per-user Steam linking, and the
  Docker 29.4.2 / CVE-2026-31431 seccomp fix for SteamCMD connectivity.

## [0.2.0]

### Added
- Persistent Workshop download and installed-item registry with exact installed
  filenames for precise uninstall.
- L4D2-oriented VPK plus same-stem JPG/JPEG/PNG install selection, with the
  default preset changed to `left4dead2/addons`.
- Installed-content scan for unmanaged files in `left4dead2/addons` and
  `left4dead2/addons/workshop`, plus import/track actions.
- Helper health and SteamCMD connectivity diagnostics surfaced in admin config.

### Changed
- Steam account selection is hidden unless the user has `calaworkshop.configure`.
- Steam Web API key copy now clarifies that SteamCMD handles downloads and the
  key is only for metadata.
- Helper and extension releases should be deployed together because the helper
  now returns selected install artifacts as transfer zips with a `files` list.

## [0.1.2]

### Security
- Restricted linked Steam account management and account-backed downloads to admins
  with `calaworkshop.configure` until per-user Steam account ownership is enforced.
- Added shared validation for server install/list/delete paths, target file names,
  and Steam account labels to reject traversal/control-character inputs before
  calling Wings or the helper.

### Fixed
- Fixed backend install-time compile errors caused by `helper_url` settings
  deserialization returning `CompactString`.
- Removed backend warnings for unused imports and unnecessary mutable bindings.
- Removed accidentally committed build logs and ignored future `*.remove.*` scratch
  logs.

## [0.1.1]

### Fixed
- Frontend build failure on install: intra-extension imports used the `@/` alias,
  which resolves to the *panel's* `frontend/src`, not the extension's own source.
  Switched all imports of the extension's own modules to relative paths (`@/` is now
  used only for panel-provided modules like `@/api/axios.ts`, `@/elements/*`).

## [0.1.0] — initial

### Added
- Per-server **Workshop** tab: paste a Workshop URL/ID, download via the helper, and
  install onto the server through Wings `files/pull` (works on AIO and remote nodes).
- Recent-downloads job tracking; installed-content listing and deletion.
- Admin configuration card: helper URL/token and Steam Web API key (encrypted), plus a
  game-preset editor (Left 4 Dead 2 seeded -> `left4dead2/addons/workshop`).
- Steam account linking (account page): SteamCMD login + Steam Guard, proxied to the
  helper.
- Permissions: server `workshop.{read,install,remove}`, user `calaworkshop.link-steam`,
  admin `calaworkshop.configure`.
- Standalone Rust **SteamCMD helper** service (static musl image, published to GHCR).
- CI (fmt/clippy/check + archive packaging) and tag-driven releases (image + `.c7s.zip`).

### Known limitations
- At this release, Workshop **search** GUI was not implemented yet (paste-only).
- Per-user ownership scoping of Steam links is scaffolded (migration table) but not
  enforced; v1 linking is a thin proxy suited to a single-admin panel.
- Anonymous downloads only work for games Valve allows; L4D2 needs a linked account.
