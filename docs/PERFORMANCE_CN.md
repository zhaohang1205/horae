# 性能测试

[README_CN.md](../README_CN.md) 中的性能数据来自以下测试流程。任何人都可以在本机针对隔离的临时配置目录复现，不会触碰 `~/.config/horae/` 下的真实数据。

## 测试环境

| 项 | 值 |
|---|---|
| 日期 | 2026-11-13 |
| 源码 | `4857868`，`cargo build --release` |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| 机器 | 11th Gen Intel(R) Core(TM) i5-11400H @ 2.70GHz (`x86_64`) |
| 系统 | Linux `7.2.15-1-omarchy` |
| 二进制体积 | `5.0 MB`（`target/release/horae`，LTO + `opt-level = "z"`，内置 SQLite） |

## 测试方法

在隔离的 `HORAE_CONFIG_DIR` 完成一次建库预热后，将每条命令作为**全新操作系统进程**连续启动 50 次（`subprocess.run`）。计时覆盖从进程启动、参数解析、SQLite 事务读写到进程退出的完整端到端壁钟耗时（`time.perf_counter()`）。

这是本机冷进程实测，仅反映真实存储与启动路径开销，不包含终端渲染器本身的绘制耗时。

## 测试结果

| 操作 | 最小 | P50 | P95 | 最大 |
|---|---:|---:|---:|---:|
| `horae --version` | 0.93 ms | **1.04 ms** | 1.17 ms | 1.26 ms |
| `horae capture`（一句话解析 + 写入时间线） | 1.57 ms | **1.88 ms** | 2.23 ms | 2.93 ms |
| `horae list --status inbox` | 1.36 ms | **1.67 ms** | 2.87 ms | 3.04 ms |
| `horae do`（专注推荐打分） | 1.53 ms | **1.82 ms** | 3.72 ms | 4.18 ms |
| `horae calendar --short` | 1.29 ms | **1.65 ms** | 2.34 ms | 4.67 ms |

## 本地复现

```sh
cargo build --release

python3 - << 'EOF'
import os, shutil, subprocess, tempfile, time

tmp = tempfile.mkdtemp(prefix="horae_perf_")
env = os.environ.copy()
env["HORAE_CONFIG_DIR"] = tmp
bin_path = "./target/release/horae"

# 预热建库与迁移
subprocess.run([bin_path, "capture", "seed warmup @work"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)

def bench(name, args, n=50):
    times = []
    for i in range(n):
        t0 = time.perf_counter()
        subprocess.run(
            [bin_path] + [a.format(i=i) for a in args],
            env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True
        )
        times.append((time.perf_counter() - t0) * 1000.0)
    times.sort()
    p50 = times[len(times) // 2]
    p95 = times[int(len(times) * 0.95)]
    print(f"{name:28s} min={times[0]:.2f} ms  p50={p50:.2f} ms  p95={p95:.2f} ms  max={times[-1]:.2f} ms")

bench("horae --version", ["--version"])
bench("horae capture (quick-add)", ["capture", "perf task {i} @work ~tomorrow 10:00 !medium"])
bench("horae list --status inbox", ["list", "--status", "inbox"])
bench("horae do (focus score)", ["do"])
bench("horae calendar --short", ["calendar", "--short"])
shutil.rmtree(tmp)
EOF
```
