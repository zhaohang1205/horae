<script lang="ts">
  import { store, VIEW_LABELS } from "../store.svelte";
  import TaskRow from "./TaskRow.svelte";
</script>

<section class="list">
  <div class="head">
    <h2 class="wordmark">{VIEW_LABELS[store.view]}</h2>
    <span class="count faint">{store.tasks.length}</span>
  </div>

  {#if store.error}
    <p class="err">⚠ {store.error}</p>
  {/if}

  <div class="rows">
    {#each store.tasks as t (t.id)}
      <TaskRow task={t} />
    {:else}
      <p class="empty faint">这一栏空空如也 — 用顶部捕获框记点什么吧。</p>
    {/each}
  </div>
</section>

<style>
  .list {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    padding: 1rem 1.4rem 0.6rem;
  }
  .head h2 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--paper);
  }
  .count {
    font-family: var(--font-mono);
    font-size: 0.9rem;
  }
  .err {
    margin: 0 1.4rem 0.5rem;
    color: var(--rose);
    font-size: 0.85rem;
  }
  .rows {
    position: relative;
    flex: 1;
    overflow-y: auto;
    padding: 0.4rem 1.1rem 2rem;
  }
  /* 贯穿任务的时间轴发丝线 */
  .rows::before {
    content: "";
    position: absolute;
    left: 1.85rem;
    top: 0.4rem;
    bottom: 2rem;
    width: 1px;
    background: linear-gradient(180deg, transparent, var(--rule) 8%, var(--rule) 92%, transparent);
  }
  .empty {
    font-family: var(--font-cjk);
    padding: 2rem 1.4rem;
    text-align: center;
  }
</style>
