CREATE TABLE IF NOT EXISTS dev_wasian_calaworkshop_download_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    server_uuid UUID NOT NULL,
    app_id INTEGER NOT NULL,
    workshop_id BIGINT NOT NULL,
    helper_job_id UUID,
    state VARCHAR(32) NOT NULL DEFAULT 'queued',
    title TEXT,
    preview_url TEXT,
    install_path TEXT,
    file_name TEXT,
    files JSONB NOT NULL DEFAULT '[]'::jsonb,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS dev_wasian_calaworkshop_download_jobs_server_updated_idx
    ON dev_wasian_calaworkshop_download_jobs (server_uuid, updated_at DESC);

CREATE TABLE IF NOT EXISTS dev_wasian_calaworkshop_installed_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    server_uuid UUID NOT NULL,
    app_id INTEGER NOT NULL,
    workshop_id BIGINT,
    title TEXT,
    install_path TEXT NOT NULL,
    vpk_file TEXT,
    image_file TEXT,
    files JSONB NOT NULL DEFAULT '[]'::jsonb,
    source VARCHAR(16) NOT NULL DEFAULT 'managed',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS dev_wasian_calaworkshop_installed_items_server_idx
    ON dev_wasian_calaworkshop_installed_items (server_uuid, install_path);
