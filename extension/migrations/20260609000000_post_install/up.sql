-- Persist the post-install behavior (`none` | `extract`) chosen from the game
-- preset at download time, so the install step is driven by server-side state
-- instead of a client-supplied flag.
ALTER TABLE dev_wasian_calaworkshop_download_jobs
    ADD COLUMN IF NOT EXISTS post_install VARCHAR(16) NOT NULL DEFAULT 'none';
