-- 维护本地 horae 任务与飞书官方任务 (Tasks v2) 的映射关系与对账状态
CREATE TABLE IF NOT EXISTS task_feishu_links (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    feishu_guid TEXT NOT NULL UNIQUE,
    last_synced_at INTEGER NOT NULL,
    sync_hash TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_feishu_links_guid ON task_feishu_links(feishu_guid);
