//! 证明 GUI command 内部逻辑与 CLI/TUI 走同一份 `horae_core::repo::*`，写库后
//! 用核心层直接读取即可得到一致结果（核心复用无重复实现）。

use horae_core::model::task::Status;
use horae_core::repo;
use horae_gui::commands::tasks::fns;

#[test]
fn gui_capture_and_transition_reuse_core_repo() {
    let tmp = tempfile::tempdir().unwrap();
    // 让 core 把配置/数据库写到临时目录，避免触碰真实 ~/.config/horae。
    std::env::set_var("HORAE_CONFIG_DIR", tmp.path());

    let conn = horae_core::db::conn::open(None).expect("open temp db");

    // GUI capture（复用 CLI 的 quick-add 解析）：含 ~时间应排程为 Scheduled。
    let t = fns::capture(&conn, "买牛奶 ~18:00 @home").expect("capture");
    assert_eq!(t.title, "买牛奶");
    assert_eq!(t.status, Status::Scheduled);
    assert!(t.scheduled_start_at.is_some());

    // 用核心层直接读取，验证 GUI 写路径落到了同一张表。
    let from_core = repo::tasks::get(&conn, &t.id).expect("core get");
    assert_eq!(from_core.id, t.id);
    assert_eq!(from_core.status, Status::Scheduled);

    // GUI transition 勾完成。
    let done = fns::transition(&conn, &t.id, "done").expect("transition");
    assert_eq!(done.status, Status::Done);

    // 核心层再次直读，证明状态已持久化、GUI 与 CLI 共享同一数据库。
    let after = repo::tasks::get(&conn, &t.id).expect("core get after");
    assert_eq!(after.status, Status::Done);

    tmp.close().unwrap();
}
