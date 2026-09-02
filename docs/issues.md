# 本地 Issue 备忘（待修复）

> 来源：2026-08-26 为 docs/skills/horae 技能包做示例验证时发现。
> 所有复现命令均在隔离环境实测通过（`HORAE_CONFIG_DIR=/tmp/xxx target/debug/horae ...`）。
> 修复合入时请同步更新 CHANGELOG.md，并视情况同步 AGENTS.md / README_cn.md 的相关表述。

---

## Issue 1（高优）：`horae pomo start` 只认完整 UUID，不支持 id 前缀/标题解析

**✅ 已修复**：见 CHANGELOG「Unreleased → Fixed」；`src/commands/pomo.rs::start`
已改走 `tasks::resolve_id`，集成测试 `tests/cli.rs::pomo_start_accepts_id_prefix`
锁定行为；skills 文档中的避坑说明已删除。

**现象**

```sh
horae pomo start 6ba2f166        # Error: task not found: 6ba2f166 （任务实际存在）
horae show 6ba2f166              # 正常工作
```

其他所有接受 `<id>` 的命令都支持「精确 id > 唯一前缀 > 唯一精确标题」的 git 式解析
（AGENTS.md "ID resolution" 规则），唯独 pomo start 不行。

**根因**

- src/commands/pomo.rs:19 使用 `tasks::get(conn, task_id)` —— 纯全 id 精确查询；
- 其他命令均先走 `tasks::resolve_id`（见 src/commands/status.rs:8、show.rs:19、
  tagging.rs:10、watch.rs:260）。

**建议修复**

pomo.rs::start 开头加一行：

```rust
let task_id = tasks::resolve_id(conn, task_id)?;
let task = tasks::get(conn, &task_id)?;
```

补一条测试：前缀启动番茄成功（参考 tests/cli.rs 里现有的 resolve 断言写法）。

**影响面**

- CLI 文档：README_cn.md 中 `horae pomo start <id>` 描述无需改，但行为将更一致；
- docs/skills/horae/SKILL.md 与 references/cli.md 中「pomo start 只认完整 UUID」的
  避坑说明在修复后应删除/改写。

---

## Issue 2（高优）：CLI 路径不校验 RRULE，`FREQ=YEARLY` 被静默存入且完成后不重排

**✅ 已修复**：
- 方案 A（校验前置）：`capture.rs::run` 与 `status.rs::schedule` 写库前均调 `parser::rrule_valid`
  （经 `commands/capture.rs::ensure_rrule_supported`），watch 手机桥复用 capture 路径自动覆盖；
- 方案 B（引擎补 YEARLY）**已实现**：`*y` / `*Ny` / `*y[jan,jul]`（BYMONTH）现在可正常解析并展开，
  `rrule_valid` 不再拒绝 YEARLY。文档（README_cn/README_en/AGENTS.md/architecture-handoff/skills）已同步。

**现象**

```sh
horae capture "年度体检 *y"
# captured [7f718bf9] 年度体检  (status: inbox)     ← TUI 同输入会被拒绝，CLI 却放行
horae done 7f718bf9
# 任务直接完成，rrule 仍是 FREQ=YEARLY，不会重排到明年 —— 年循环静默退化成一次性任务

horae schedule <id> --start 明天 --rrule "FREQ=YEARLY"
# scheduled ... rrule: FREQ=YEARLY                  ← schedule 子命令同样不校验
```

引擎现在支持 `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY`（src/schedule.rs），YEARLY 通过
`*y` / `*Ny` / `*y[jan,jul]`（BYMONTH）展开。`rrule_valid` 已放开 YEARLY，CLI 与 TUI
在所有写入路径统一校验，不再静默降级。

**根因（历史）**

- 原 `crate::parser::rrule_valid` 内含 YEARLY 拒绝逻辑，只有 TUI 在调用；
- CLI capture 路径原只做 `parse_quick_add`，简写展开结果未经 `rrule_valid` 直接入库；
- CLI schedule 路径原把 `--rrule` 原样传给 `tasks::schedule`，无校验。

上述根因已在方案 A 中消除（CLI 两入口统一校验），并在方案 B 中进一步让 YEARLY 本身合法可展开。

**附带文档不一致（已同步）**

- docs/AGENTS.md / README_cn / README_en / architecture-handoff / skills 文档已改为
  YEARLY 受支持的表述；CLI 与 TUI 在所有写入路径一致校验。

---

## Issue 3（中优）：v0.1.0 Release 未附带预编译二进制，打包 workflow 上线后未重新发版

**✅ 已修复**：随 v0.1.1 重新发版触发 release.yml（见 CHANGELOG）；v0.1.0 上的
损坏 `gtp` 资产已删除；install.md「路径 A」的 ⚠️ 说明已改写。

**现象**

- GitHub Releases/latest（v0.1.0）资产仅有 Source code zip/tar.gz 和一个损坏的
  `gtp` 残留链接；按当前 release.yml 命名规则推测的
  `horae-v0.1.0-<target>.tar.gz` 全部 404（已实测）。
- 用户无法走"预编译二进制"安装路径，只能 `cargo install --git` 自行编译。

**根因**

- v0.1.0 tag 恰好指向 47edbd1——该提交**引入了**旧版单 job release.yml；
  多平台矩阵构建 + 打包上传（5 target + sha256）是 tag 之后才在 main 演进成型的。
  即：新版打包 workflow 从未随任何 tag 跑过。

**建议修复**

打一个新 tag（如 v0.1.1 或 v0.2.0）触发当前 release.yml，验证 Release 页出现：

```text
horae-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz(+.sha256)
horae-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz(+.sha256)
horae-vX.Y.Z-aarch64-apple-darwin.tar.gz(+.sha256)
horae-vX.Y.Z-x86_64-apple-darwin.tar.gz(+.sha256)
horae-vX.Y.Z-x86_64-pc-windows-msvc.zip(+.sha256)
```

顺带清理 v0.1.0 上损坏的 `gtp` 资产（编辑 release 删除即可）。

**影响面**

- docs/skills/horae/references/install.md「路径 A」的 ⚠️ 现状说明可删除/改写；
- docs/skills/horae/scripts/preflight.sh 的 Release 资产探测将自动报告可用。

## 验证环境备忘

```sh
export HORAE_CONFIG_DIR=/tmp/opencode/horae-test   # 隔离数据目录，随时可删
cargo build && target/debug/horae capture "年度体检 *y"
```
