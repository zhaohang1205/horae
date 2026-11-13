# Performance Benchmark

The numbers in [README.md](../README.md) come from the measurement procedure below. Anyone can reproduce them locally against an isolated temporary config directory without touching `~/.config/horae/`.

## Environment

| Item | Value |
|---|---|
| Date | 2026-11-13 |
| Commit | `4857868`, `cargo build --release` |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| Machine | 11th Gen Intel(R) Core(TM) i5-11400H @ 2.70GHz (`x86_64`) |
| OS | Linux `7.2.15-1-omarchy` |
| Binary size | `5.0 MB` (`target/release/horae`, LTO + `opt-level = "z"`, SQLite bundled) |

## Methodology

Each command is spawned **50 consecutive times** as a fresh OS process (`subprocess.run`) after an initial database warm-up in an isolated `HORAE_CONFIG_DIR`. Timings are wall-clock process execution times (`time.perf_counter()`), including process startup, CLI parsing, SQLite open/query/commit, and process exit.

This measures single-machine local CLI/storage latency, not terminal emulator draw latency or cross-tool comparisons on different workloads.

## Results

| Operation | Min | P50 | P95 | Max |
|---|---:|---:|---:|---:|
| `horae --version` | 0.93 ms | **1.04 ms** | 1.17 ms | 1.26 ms |
| `horae capture` (quick-add + timeline write) | 1.57 ms | **1.88 ms** | 2.23 ms | 2.93 ms |
| `horae list --status inbox` | 1.36 ms | **1.67 ms** | 2.87 ms | 3.04 ms |
| `horae do` (focus scoring) | 1.53 ms | **1.82 ms** | 3.72 ms | 4.18 ms |
| `horae calendar --short` | 1.29 ms | **1.65 ms** | 2.34 ms | 4.67 ms |

## Reproduce

```sh
cargo build --release

python3 - << 'EOF'
import os, shutil, subprocess, tempfile, time

tmp = tempfile.mkdtemp(prefix="horae_perf_")
env = os.environ.copy()
env["HORAE_CONFIG_DIR"] = tmp
bin_path = "./target/release/horae"

# Warm up SQLite migrations once
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
