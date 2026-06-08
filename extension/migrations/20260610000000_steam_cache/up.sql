CREATE TABLE IF NOT EXISTS dev_wasian_calaworkshop_steam_cache (
    namespace text NOT NULL,
    cache_key text NOT NULL,
    value jsonb NOT NULL,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (namespace, cache_key)
);

CREATE INDEX IF NOT EXISTS dev_wasian_calaworkshop_steam_cache_expires_idx
    ON dev_wasian_calaworkshop_steam_cache (expires_at);
