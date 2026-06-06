# Changelog

All notable changes to this project are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/); versions are tag-driven.

## [Unreleased]

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
  game-preset editor (Left 4 Dead 2 seeded → `left4dead2/addons/workshop`).
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
