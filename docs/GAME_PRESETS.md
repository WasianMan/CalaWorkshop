# Game Presets

CalaWorkshop ships tested presets for **Left 4 Dead 2** and **Garry's Mod**.
Other games can be added from the admin settings page without changing code, as
long as their Workshop install can be described as "download with SteamCMD, then
copy, rename, extract, or generate files into the server volume".

There are two JSON shapes:

- Full preset JSON, stored by the backend and shown in
  [`games.example.json`](./games.example.json). This includes `app_id`, `name`,
  and `install_path`.
- Advanced JSON, edited in the per-preset **Advanced (JSON)** box and shown in
  [`advanced-rule.example.json`](./advanced-rule.example.json). This includes
  only the rule fields for one existing preset.

## Full Preset Fields

```json
{
  "app_id": 4000,
  "name": "Garry's Mod",
  "install_path": "garrysmod",
  "auth": "anonymous",
  "match": [],
  "extract_files": [],
  "generated_files": [],
  "scan": [],
  "post_install": "none"
}
```

- `app_id`: Steam app id used with `workshop_download_item`.
- `name`: Display name in the Workshop tab picker.
- `install_path`: Server-volume root where Wings places the helper artifact.
- `auth`: `default`, `anonymous`, or `account`.
- `match`: Select downloaded files with globs and optionally rename them.
- `extract_files`: Extract supported payloads into a destination folder.
  Currently supported format: `gma`.
- `generated_files`: Add small templated files to the install artifact.
- `scan`: Paths the Installed Content page scans for unmanaged content.
- `post_install`: `none` or `extract` for nested archives after placement.

## Advanced JSON Fields

Use camelCase in the UI's **Advanced (JSON)** box:

```json
{
  "auth": "anonymous",
  "match": [],
  "extractFiles": [],
  "generatedFiles": [],
  "scan": [],
  "postInstall": "none"
}
```

Do not paste a full preset object into the Advanced box. The App ID, name, and
install path are edited in the structured row above it.

## Template Tokens

These tokens work in `match.rename`, `extractFiles.to`, and generated file paths
or content:

- `{workshop_id}`: Workshop item id.
- `{app_id}`: Steam app id.
- `{title_slug}`: Steam title converted to a safe lowercase-ish filename slug.
- `{basename}`: Original downloaded filename without extension.
- `{ext}`: Original downloaded extension, lowercased.

Rendered paths must stay relative to `install_path`; absolute paths, `..`, drive
paths, and control characters are rejected.

## Common Patterns

Copy all downloaded files as-is:

```json
{
  "auth": "anonymous",
  "match": [],
  "extractFiles": [],
  "generatedFiles": [],
  "scan": [],
  "postInstall": "none"
}
```

Rename one downloaded payload:

```json
{
  "auth": "account",
  "match": [
    { "glob": "*.vpk|*_legacy.bin", "rename": "{workshop_id}.vpk" }
  ],
  "extractFiles": [],
  "generatedFiles": [],
  "scan": [
    { "path": "left4dead2/addons", "extensions": ["vpk"] }
  ],
  "postInstall": "none"
}
```

Extract a Garry's Mod addon and generate client-delivery Lua:

```json
{
  "auth": "anonymous",
  "match": [],
  "extractFiles": [
    {
      "format": "gma",
      "glob": "*.gma|*_legacy.bin",
      "to": "addons/{title_slug}_{workshop_id}"
    }
  ],
  "generatedFiles": [
    {
      "path": "lua/autorun/server/cala_workshop_{workshop_id}.lua",
      "content": "if SERVER then resource.AddWorkshop(\"{workshop_id}\") end\n"
    }
  ],
  "scan": [
    { "path": "garrysmod/addons", "extensions": [] }
  ],
  "postInstall": "none"
}
```

## Limits

The manifest is intentionally powerful but not magic. It works best for games
where a Workshop item can be installed by placing files in known paths. Games
that require server startup flags, collection management, database changes, or
game-specific config rewrites may need a new preset pattern or future code
support.

Preset editing is admin-only because `generatedFiles` can create game-server
scripts, such as Garry's Mod Lua.
