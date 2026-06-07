-- Maps a Calagopus user to a Steam account label cached on the helper.
-- Created now as forward-looking scaffolding; per-user ownership scoping of
-- Steam links is wired up in a later phase (see CONTRACT.md / README roadmap).
CREATE TABLE IF NOT EXISTS dev_wasian_calaworkshop_steam_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_uuid UUID NOT NULL,
    label VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_uuid, label)
);
