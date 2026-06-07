DROP INDEX IF EXISTS dev_wasian_calaworkshop_steam_links_helper_label_idx;
ALTER TABLE dev_wasian_calaworkshop_steam_links
    DROP COLUMN IF EXISTS helper_label,
    DROP COLUMN IF EXISTS steam_username,
    DROP COLUMN IF EXISTS updated_at;
