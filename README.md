# calaworkshop

[![ci](https://github.com/OWNER/calaworkshop/actions/workflows/ci.yml/badge.svg)](https://github.com/OWNER/calaworkshop/actions/workflows/ci.yml)
[![release](https://github.com/OWNER/calaworkshop/actions/workflows/release.yml/badge.svg)](https://github.com/OWNER/calaworkshop/actions/workflows/release.yml)
[![license: MIT + Commons Clause](https://img.shields.io/badge/license-MIT%20%2B%20Commons%20Clause-blue.svg)](./LICENSE)

> Replace `OWNER` in the badge URLs above with your GitHub username/org after you push.

A **Steam Workshop downloader for [Calagopus](https://calagopus.com)**, shipped as a
panel **extension** (`dev.wasian.calaworkshop`) plus a small **SteamCMD helper**
service. Adds a per-server **Workshop** tab: paste a Workshop URL/ID (search is on
the roadmap) and it installs the content straight onto your game server.

Built for Left 4 Dead 2 first, but works for any Steam game via configurable presets.

> **Status: alpha, and a side project.** This is something I build and run for my own
> server and share in case it's useful. I genuinely intend to get it stable and
> polished, but I can't promise active or long-term maintenance, fast issue responses,
> or backwards compatibility between alpha releases. Use it at your own risk — there is
> **no warranty or guarantee of any kind** (see [LICENSE](./LICENSE)). Bug reports and
> PRs are welcome, and I'll get to them when I can.

## How it works

```
Workshop tab  ──>  extension backend  ──POST──>  helper (runs SteamCMD)
   (React)          (Rust, in panel)                 │ downloads item, serves it at /files
                          │                          ▼
                          └── Wings files/pull  <── http://helper:8090/files/<job>?token=…
                                   places the file into the server volume
```

The extension never runs SteamCMD and never touches a server volume directly. It
asks the helper to download, then tells **Wings** to pull the helper's file URL into
the server. Because Wings does the placement, this works on AIO **and** remote nodes.

Full design: [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) · wire format:
[CONTRACT.md](./CONTRACT.md).

## Quick start

You need the Calagopus **heavy** image (`:heavy`/`:heavy-aio`) — the regular images
can't compile extensions. Then:

1. Add the helper service to your compose and pull its published image
   (`ghcr.io/OWNER/calaworkshop-helper:latest`).
2. Switch the panel to the heavy image and add the four build mounts.
3. Drop `dev_wasian_calaworkshop.c7s.zip` (from the latest
   [Release](https://github.com/OWNER/calaworkshop/releases)) into the panel's
   `/app/extensions` mount and restart.
4. Configure the helper URL/token and game presets in the admin panel.

Step-by-step (including Coolify): **[docs/DEPLOY.md](./docs/DEPLOY.md)**.

## ⚠️ Steam auth reality

There is **no passwordless download token**:

- A **Steam Web API key** is a real token but only powers search/metadata.
- Downloading owned/private content requires a **SteamCMD login** (username +
  password + a one-time Steam Guard code); the helper caches the session.
- **Anonymous downloads only work for some games. Left 4 Dead 2 (550) requires an
  account that owns the game** — link one on the Steam Link page.

## Repository layout

```
calaworkshop/
├── extension/              # the .c7s.zip contents (Metadata.toml + backend/ + frontend/ + migrations/)
│   ├── backend/            # Rust extension (routes, settings, helper client)
│   ├── frontend/           # React UI (Workshop tab, Steam link, admin config)
│   └── migrations/
├── helper/                 # standalone Rust SteamCMD service + Dockerfile
├── packaging/              # build-c7s.ps1 (Windows) / build-c7s.sh (Linux/CI)
├── docs/                   # DEPLOY, ARCHITECTURE
├── compose.aio.example.yml # reference AIO compose with the helper wired in
├── CONTRACT.md             # extension ⇄ helper HTTP contract
└── .github/workflows/      # ci + release (builds image to GHCR, publishes .c7s.zip)
```

## Permissions

| Scope  | Node                       | Allows |
| ------ | -------------------------- | ------ |
| server | `workshop.read`            | See the Workshop tab + installed content |
| server | `workshop.install`         | Download & install items |
| server | `workshop.remove`          | Delete installed content |
| user   | `calaworkshop.link-steam`  | Link/manage Steam accounts on the helper |
| admin  | `calaworkshop.configure`   | Edit helper connection, API key, presets |

## Building locally

- Helper: `cd helper && cargo build` (or `WORKSHOP_HELPER_TOKEN=dev cargo run`).
- Extension archive: `packaging/build-c7s.ps1` (Windows) or `packaging/build-c7s.sh` (Linux).
- The extension **backend** only compiles inside the Calagopus panel workspace (it
  depends on the `shared`/`wings-api` crates); it's validated at heavy-image install
  time. See [CONTRIBUTING.md](./CONTRIBUTING.md).

## Status / roadmap

**Current: `v0.1.0` — alpha.** Functional end-to-end, but expect rough edges.

Working: per-server Workshop tab; paste URL/ID → download → Wings-pull install; job
tracking; installed-content list + delete; admin config with encrypted secrets;
Steam account linking (login + Guard) proxied to the helper.

Planned: search GUI (`IPublishedFileService/QueryFiles`) + collection expansion;
per-user ownership scoping of Steam links; update/reinstall actions; richer item
previews; multi-node helper-reachability guidance.

## License

**MIT + [Commons Clause](https://commonsclause.com/)** — free for personal and
internal use; you may not *sell* it or offer a hosted/commercial service based on it
without a commercial license. See [LICENSE](./LICENSE). Commercial inquiries:
`adam@wasian.dev`.

> Source-available, not OSI "open source" (it restricts commercial selling).
