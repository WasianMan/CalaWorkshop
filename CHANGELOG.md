# Changelog

All notable changes to this project are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/); versions are tag-driven.

## [Unreleased]

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
- Workshop **search** GUI is not implemented yet (paste-only).
- Per-user ownership scoping of Steam links is scaffolded (migration table) but not
  enforced; v1 linking is a thin proxy suited to a single-admin panel.
- Anonymous downloads only work for games Valve allows; L4D2 needs a linked account.
