# Contributing

Thanks for your interest! calaworkshop has two independently-buildable parts.

## Helper (standalone Rust)

```bash
cd helper
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo check
WORKSHOP_HELPER_TOKEN=dev cargo run     # listens on 0.0.0.0:8090
```

CI runs `fmt --check`, `clippy -D warnings`, and `cargo check` on every push/PR — keep
it clean. The image build uses a static musl target (see `helper/Dockerfile`).

## Extension

The `backend/` crate depends on the Calagopus `shared` and `wings-api` workspace
crates, so it **only compiles inside a Calagopus panel checkout / dev environment** —
not standalone. The two ways to work on it:

- **Dev environment:** clone the panel, then
  ```bash
  panel-rs extensions add path/to/dev_wasian_calaworkshop.c7s.zip
  panel-rs extensions apply --profile dev
  ```
- **Heavy image:** drop the `.c7s.zip` into the panel's `/app/extensions` mount and
  restart (it recompiles on startup).

API references used while building: <https://calagopus.com/ai-doc/extensions.md>.

## Packaging the extension archive

The archive must contain explicit zip **directory entries** or the panel rejects it.
Use the provided scripts (don't `Compress-Archive` by hand):

```bash
# Linux / CI
bash packaging/build-c7s.sh
```
```powershell
# Windows
./packaging/build-c7s.ps1
```
Output: `dist/dev_wasian_calaworkshop.c7s.zip`.

## Cutting a release

Releases are tag-driven. Bump versions, then:

```bash
git tag v0.2.0
git push origin v0.2.0
```

The `release` workflow:
1. builds & pushes `ghcr.io/<owner>/calaworkshop-helper` (`:x.y.z`, `:x.y`, `:latest`),
2. builds the `.c7s.zip` and attaches it to a generated GitHub Release.

Bump `helper/Cargo.toml`, `extension/backend/Cargo.toml`, and
`extension/frontend/package.json` versions together, and update `CHANGELOG.md`.

## Conventions

- Match the surrounding code style; run formatters before committing.
- Extension routes follow the file-tree-mirrors-URL-tree convention (a `mod.rs` per
  directory; `_param_` files for path parameters).
- Frontend request bodies are sent **snake_case** (the panel does not transform request
  bodies, only responses).

## License

By contributing you agree your contributions are licensed under the repository's
**MIT + Commons Clause** license (see [LICENSE](./LICENSE)).
