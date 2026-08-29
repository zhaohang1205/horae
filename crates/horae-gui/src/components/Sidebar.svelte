<script lang="ts">
  import { onMount } from "svelte";
  import { store, VIEW_LABELS, type ViewName } from "../store.svelte";

  let now = $state(new Date());
  onMount(() => {
    const t = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(t);
  });

  const views: ViewName[] = [
    "today",
    "inbox",
    "next",
    "scheduled",
    "waiting",
    "someday",
    "reference",
    "archived",
    "all",
  ];

  // 模拟时钟指针角度
  const sec = now.getSeconds() + now.getMilliseconds() / 1000;
  const min = now.getMinutes() + sec / 60;
  const hr = (now.getHours() % 12) + min / 60;
  const sa = sec * 6;
  const ma = min * 6;
  const ha = hr * 30;

  function openReview() {
    store.reviewOpen = true;
    store.reviewStep = 0;
  }
</script>

<aside class="sidebar">
  <div class="clock">
    <svg viewBox="0 0 100 100" width="56" height="56">
      <circle cx="50" cy="50" r="47" fill="var(--ink-800)" stroke="var(--rule)" stroke-width="2" />
      {#each Array(12) as _, i}
        <line
          x1="50"
          y1="6"
          x2="50"
          y2={i % 3 === 0 ? 13 : 10}
          stroke="var(--paper-faint)"
          stroke-width={i % 3 === 0 ? 2 : 1}
          transform="rotate({i * 30} 50 50)"
        />
      {/each}
      <line x1="50" y1="50" x2="50" y2="30" stroke="var(--paper)" stroke-width="3" stroke-linecap="round" transform="rotate({ha} 50 50)" />
      <line x1="50" y1="50" x2="50" y2="22" stroke="var(--paper)" stroke-width="2" stroke-linecap="round" transform="rotate({ma} 50 50)" />
      <line x1="50" y1="50" x2="50" y2="18" stroke="var(--amber)" stroke-width="1.6" stroke-linecap="round" transform="rotate({sa} 50 50)" />
      <circle cx="50" cy="50" r="3" fill="var(--amber)" />
    </svg>
    <div class="clock-cap faint">horae · 时序</div>
  </div>

  <nav>
    {#each views as v}
      <button class="nav" class:active={store.view === v} onclick={() => {
        store.view = v;
        store.refresh();
      }}>
        <span class="dot"></span>{VIEW_LABELS[v]}
      </button>
    {/each}
  </nav>

  <button class="review" onclick={openReview}>
    <span class="amber">↻</span> 周回顾
  </button>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 1rem 0.7rem;
    border-right: 1px solid var(--rule);
    background: linear-gradient(180deg, rgba(0, 0, 0, 0.2), transparent);
    overflow-y: auto;
  }
  .clock {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.2rem;
    padding-bottom: 0.6rem;
    border-bottom: 1px solid var(--rule-soft);
  }
  .clock-cap {
    font-family: var(--font-cjk);
    font-size: 0.7rem;
    letter-spacing: 0.2em;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    justify-content: flex-start;
    border: none;
    border-radius: var(--radius-sm);
    padding: 0.5rem 0.7rem;
    color: var(--paper-dim);
    font-family: var(--font-cjk);
    font-size: 0.95rem;
    background: transparent;
  }
  .nav:hover {
    background: var(--ink-700);
    color: var(--paper);
  }
  .nav.active {
    background: var(--ink-700);
    color: var(--amber);
  }
  .nav .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--rule);
  }
  .nav.active .dot {
    background: var(--amber);
    box-shadow: 0 0 8px var(--amber);
  }
  .review {
    margin-top: auto;
    border: 1px dashed var(--amber-soft);
    color: var(--amber);
    font-family: var(--font-cjk);
    padding: 0.55rem;
  }
</style>
