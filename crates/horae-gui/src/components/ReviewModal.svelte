<script lang="ts">
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import { store } from "../store.svelte";
  import TaskList from "./TaskList.svelte";
  import type { ViewName } from "../store.svelte";

  // 引导式 4 步：收集箱 → 等待中 → 将来也许 → 已完成（对齐 TUI 的回顾流程）
  const STEPS: { view: ViewName; name: string }[] = [
    { view: "inbox", name: "收集箱 · 清空大脑" },
    { view: "waiting", name: "等待中 · 跟进项" },
    { view: "someday", name: "将来也许 · 孵化" },
    { view: "done", name: "已完成 · 庆祝" },
  ];

  onMount(() => {
    store.reviewStep = 0;
    goStep(0);
  });

  async function goStep(i: number) {
    store.reviewStep = i;
    store.view = STEPS[i].view;
    await store.refresh();
  }

  function next() {
    if (store.reviewStep >= STEPS.length - 1) {
      store.reviewOpen = false;
      store.view = "next";
      store.refresh();
      return;
    }
    goStep(store.reviewStep + 1);
  }
  function close() {
    store.reviewOpen = false;
    store.view = "next";
    store.refresh();
  }
</script>

<div
  class="scrim"
  role="button"
  tabindex="-1"
  aria-label="退出周回顾"
  onclick={close}
  onkeydown={(e) => e.key === "Escape" && close()}
></div>
<div class="modal" transition:fly={{ y: 30, duration: 300, opacity: 0.3 }}>
  <div class="banner">
    <span class="kicker faint">每周回顾</span>
    <span class="step">第 {store.reviewStep + 1} / {STEPS.length} 步</span>
    <div class="bar">
      {#each STEPS as s, i}
        <span class="seg" class:on={i <= store.reviewStep}></span>
      {/each}
    </div>
    <h2 class="wordmark">{STEPS[store.reviewStep].name}</h2>
  </div>

  <div class="pane">
    <TaskList />
  </div>

  <div class="ctrls">
    <button onclick={close}>退出</button>
    <button class="next" onclick={next}>
      {store.reviewStep >= STEPS.length - 1 ? "完成 ✦" : "下一步 →"}
    </button>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 30;
  }
  .modal {
    position: fixed;
    inset: 6vh 8vw;
    z-index: 31;
    background: var(--ink-800);
    border: 1px solid var(--rule);
    border-radius: var(--radius);
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .banner {
    padding: 1rem 1.4rem 0.8rem;
    border-bottom: 1px solid var(--rule);
    background: radial-gradient(120% 100% at 50% -20%, rgba(232, 176, 75, 0.1), transparent);
  }
  .kicker {
    font-family: var(--font-cjk);
    letter-spacing: 0.3em;
    font-size: 0.75rem;
  }
  .step {
    float: right;
    font-family: var(--font-mono);
    color: var(--amber);
    font-size: 0.85rem;
  }
  .bar {
    display: flex;
    gap: 0.4rem;
    margin: 0.6rem 0;
  }
  .seg {
    height: 4px;
    flex: 1;
    border-radius: 4px;
    background: var(--rule);
    transition: background 0.3s;
  }
  .seg.on {
    background: var(--amber);
  }
  .banner h2 {
    margin: 0;
    font-size: 1.4rem;
    color: var(--paper);
  }
  .pane {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .pane :global(.list) {
    flex: 1;
  }
  .ctrls {
    display: flex;
    justify-content: flex-end;
    gap: 0.6rem;
    padding: 0.7rem 1.4rem;
    border-top: 1px solid var(--rule);
  }
  .ctrls .next {
    border-color: var(--amber-soft);
    color: var(--amber);
  }
</style>
