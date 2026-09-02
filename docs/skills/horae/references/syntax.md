# horae 语法参考：quick-add / 时间 / 循环

一句话里混排四种前缀 token，空格分词、逐词识别：

```
标题 @标签 ~时间 *循环简写 !优先级
```

示例：

```sh
horae capture "给妈妈订蛋糕 @calls ~周六 10:00 !medium"
horae capture "站会 @work *weekday"
horae capture "灵感：少即是多 @quote"          # 配合金句功能
horae capture "对账 @computer *m[-1] ~21:00"  # 每月末，见下方循环语法
```

## 词法规则（实测）

- 按空白分词，**整词**匹配前缀；单个 `@`、`~` 等不构成 token（长度 >1 才生效），
  会被当普通标题文字。
- 四种 token 在标题中的**位置任意**，解析后从标题中剔除。
- `!high`→high（最高）、`!medium`→medium、`!low`→low（大小写均可），存为独立 `priority` 字段（CLI 也可用 `--high`/`--medium`/`--low`/`--clear-priority`）。
- 标签自动创建：`@自定义` 首次使用即建档。

## 时间语法（`~time` 与所有命令的 TIME 参数通用）

| 写法 | 含义 |
| --- | --- |
| `now` | 此刻 |
| `+2h` `+30m` `+1d` `+1w` | 相对偏移 |
| `+3d 15:30` | 相对天数 + 当日时刻 |
| `今天` `明天` `后天` [HH:MM] | 中文天词（缺省时刻则当日全天/零点） |
| `周三` `下周五` [HH:MM] | 星期几（可带"下周"，自动取未来最近一天） |
| `8/20 15:30` · `2026.8.20` | 斜杠/点分日期 |
| `HH:MM` | 今日该时刻（已过则顺延明日） |
| `2026-07-24 [HH:MM]` | 绝对日期 |

语义区别（重要）：

- **`~time` 设排程起点**（`scheduled_start_at`），任务状态进入"已排程"；只设起点不设终点。
- **`--due` 设软截止**（`due_at`）。两者可同时设置：
  `horae schedule <id> --start 明天 --end 明天 14:00`

## 循环 RRULE

标准格式（`--rrule` 参数或 TUI 编辑）：

```
FREQ=DAILY|WEEKLY|MONTHLY|YEARLY;INTERVAL=2;BYDAY=MO,WE;BYMONTHDAY=1,-1;BYMONTH=1,7;COUNT=10|UNTIL=YYYY-MM-DD
```

- `BYMONTHDAY=-1` = 月末最后一天；`COUNT` 与 `UNTIL` 二选一。
- 引擎支持 **DAILY/WEEKLY/MONTHLY|YEARLY**（366 天展开视野，进程内实现，无外部 crate）。

### 快捷简写（`*` 开头，quick-add 与 Tab 补全可用）

| 简写 | 展开为 |
| --- | --- |
| `*d` / `*w` / `*m` | 每天 / 每周 / 每月 |
| `*weekday` | 工作日（周一到周五） |
| `*weekend` | 周末 |
| `*2w[1,3]` | 每 2 周的周一、周三（数字 1-7=周一至周日，0=周日） |
| `*1w[mo,we]` | 同上，星期也可用字母代码 |
| `*m[1,15]` | 每月 1 号和 15 号 |
| `*m[1,-1]` | 每月 1 号和月末最后一天 |
| `*Nd` / `*Nw` / `*Nm` | 每 N 天/周/月 |
| `*y` | 每年 |
| `*Ny` | 每 N 年（如 `*2y` 每两年） |
| `*y[jan,jul]` | 每年 1 月与 7 月（`BYMONTH`，月份名或 1-12，可加间隔 `*2y[6]`） |

## 组合速查

| 需求 | 一句话 |
| --- | --- |
| 今天下班前要交 | `写周报 @work ~今天 18:00 !high` |
| 每两周周一站会 | `站会 @work *2w[1] ~周一 09:30` |
| 每月底对账 | `对账 @computer *m[-1] ~21:00` |
| 每年体检 | `年度体检 *y` |
| 每年 1 月与 7 月缴费 | `缴费 *y[jan,jul]` |
| 高优但没想好时间 | `联系牙医 @calls !high`（留在收件箱待厘清） |
| 收藏金句 | `horae capture "少即是多" --tag quote --status reference` |
