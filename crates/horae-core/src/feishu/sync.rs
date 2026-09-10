//! 飞书任务 (Tasks API) 与日历 (Calendar API) 数据同步扩展接口预留。
//!
//! 本模块为后续将 horae 任务双向或单向同步至飞书原生应用预留契约定义。

use anyhow::Result;
use rusqlite::Connection;

/// 飞书双向/单向同步器契约（后续实现）。
pub trait FeishuSyncEngine {
    /// 拉取飞书任务并合并至本地 SQLite
    fn pull_remote_tasks(&self, conn: &Connection) -> Result<usize>;
    /// 将本地任务推送同步至飞书官方任务/日历
    fn push_local_tasks(&self, conn: &Connection) -> Result<usize>;
}
