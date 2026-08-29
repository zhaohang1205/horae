<script lang="ts">
  import { store } from "../store.svelte";
  import { notifySystem } from "../notifications";
  import type { PomoPhase } from "../types";

  let remaining = $state(0);
  let total = $state(1);
  let firing = $state(false);
  let timer: number | undefined;

  const LABEL: Record<PomoPhase, string> = {
    idle: "待命",
    work: "专注",
    shortbreak: "小休",
    longbreak: "长休",
  };

  function phaseTotal(phase: PomoPhase, p = store.pomo): number {
    if (!p) return 1;
    const c = p.config;
    switch (phase) {
      case "work":
        return c.work_mins * 60000;
      case "shortbreak":
        return c.short_break_mins * 60000;
      case "longbreak":
        return c.long_break_mins * 60000;
      default:
        return 1;
    }
  }

  async function advance() {
    if (firing) return;
    firing = true;
    const before = store.pomo?.phase;
    const np = await store.doCompletePomo();
    if (before === "work") {
      if (np.phase === "idle") notifySystem("🍅 番茄钟结束", "本轮专注完成，休息已结清。");
      else notifySystem("🎯 专注达成！", `开始${np.phase === "longbreak" ? "长" : "小"}休 ${np.config.long_break_mins && np.phase === "longbreak" ? np.config.long_break_mins : np.config.short_break_mins} 分钟`);
    } else if (before === "shortbreak" || before === "longbreak") {
      notifySystem("⏰ 休息结束", "再接再厉，开启新一轮专注。");
    }
    firing = false;
  }

  function tick() {
    const p = store.pomo;
    if (!p || p.phase === "idle" || p.end_ts == null) {
      remaining = 0;
      return;
    }
    const left = p.end_ts - Date.now();
    remaining = Math.max(0, left);
    total = phaseTotal(p.phase);
    if (left <= 0) advance();
  }

  $effect(() => {
    const active = store.pomo && store.pomo.phase !== "idle" && store.pomo.end_ts != null;
    clearInterval(timer);
    if (active) {
      tick();
      timer = window.setInterval(tick, 250);
    }
    return () => clearInterval(timer);
  });

  const mm = $derived(String(Math.floor(remaining / 60000)).padStart(2, "0"));
  const ss = $derived(String(Math.floor((remaining % 60000) / 1000)).padStart(2, "0"));
  const R = 16;
  const C = 2 * Math.PI * R;
  const frac = $derived(total > 0 ? remaining / total : 0);
</script>

<footer class="pomo">
  <div class="ring">
    <svg viewBox="0 0 40 40" width="38" height="38">
      <circle cx="20" cy="20" r={R} fill="none" stroke="var(--rule)" stroke-width="3" />
      <circle
        cx="20"
        cy="20"
        r={R}
        fill="none"
        stroke="var(--amber)"
        stroke-width="3"
        stroke-linecap="round"
        stroke-dasharray={C}
        stroke-dashoffset={C * (1 - frac)}
        transform="rotate(-90 20 20)"
      />
    </svg>
    {#if store.pomo && store.pomo.phase !== "idle"}
      <span class="t">{mm}:{ss}</span>
    {/if}
  </div>

  <div class="info">
    <div class="phase">
      <span class="amber">🍅</span>
      {store.pomo ? LABEL[store.pomo.phase] : "待命"}
      {#if store.pomo?.task_title}
        <span class="ttl faint">· {store.pomo.task_title}</span>
      {/if}
    </div>
    {#if store.pomo}
      <div class="stat faint">
        今日 {store.pomo.today_count} · 合计 {store.pomo.total_count} · 连击 {store.pomo.streak}
      </div>
    {/if}
  </div>

  {#if store.pomo && store.pomo.phase !== "idle"}
    <button class="stop" onclick={() => store.doStopPomo()}>停止</button>
  {:else}
    <span class="hint faint">在任务详情点「开始番茄」</span>
  {/if}
</footer>

<style>
  .pomo {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    padding: 0.5rem 1.1rem;
    border-top: 1px solid var(--rule);
    background: linear-gradient(0deg, rgba(0, 0, 0, 0.25), transparent);
  }
  .ring {
    position: relative;
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
  }
  .ring .t {
    position: absolute;
    font-family: var(--font-mono);
    font-size: 0.6rem;
    color: var(--paper);
  }
  .info {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .phase {
    font-family: var(--font-cjk);
    font-size: 0.95rem;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .ttl {
    font-size: 0.85rem;
  }
  .stat {
    font-family: var(--font-mono);
    font-size: 0.72rem;
  }
  .stop {
    border-color: var(--rose);
    color: var(--rose);
  }
  .hint {
    font-family: var(--font-cjk);
    font-size: 0.8rem;
  }
</style>
