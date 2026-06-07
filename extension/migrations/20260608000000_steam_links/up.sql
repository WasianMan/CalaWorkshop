-- Per-user Steam account ownership.
--
-- The init migration created `dev_wasian_calaworkshop_steam_links` as forward
-- looking scaffolding. This migration makes it functional: each linked account
-- is owned by a single Calagopus user and maps a user-facing `label` to an
-- opaque `helper_label`. The helper stores its SteamCMD session under the opaque
-- label, so two users can pick the same friendly label without colliding and no
-- user can address another user's cached session.
ALTER TABLE dev_wasian_calaworkshop_steam_links
    ADD COLUMN IF NOT EXISTS helper_label TEXT,
    ADD COLUMN IF NOT EXISTS steam_username TEXT,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE UNIQUE INDEX IF NOT EXISTS dev_wasian_calaworkshop_steam_links_helper_label_idx
    ON dev_wasian_calaworkshop_steam_links (helper_label);
