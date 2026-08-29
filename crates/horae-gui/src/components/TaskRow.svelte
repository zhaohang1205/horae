<script lang="ts">
  import { onMount } from "svelte";
  import { store } from "../store.svelte";
  import { getTaskTags } from "../lib";
  import type { Task, Tag } from "../types";
  import { fmtDateTime, fmtRelative } from "../format";

  let { task }: { task: Task } = $props();

  let tags = $state<Tag[]>([]);
  onMount(async () => {
    try {
      tags = await getTaskTags(task.id);
    } catch {
      /* 忽略 */
    }
  });

  // 时间轴节点：取有效到期（due 优先，其次 scheduled_start）
  const due = $derived(task.due_at ?? task.scheduled_start_at);
  const dueClass = $derived.by(() => {
    const v = due;
    if (v == null) return "dim";
    const diff = v - Date.now();
    if (diff < 0) return "over";
    if (diff < 24 * 3600 * 1000) return "soon";
    return "dim";
  });

  function toggle() {
    store.doToggle(task.id, task.status);
  }
  function open() {
    store.select(task.id);
  }
  function schedule() {
    store.select(task.id);
  }
  function archive() {
    store.doArchive(task.id);
  }
</script>

<div class="row" class:selected={store.selectedId === task.id} class:done={task.status === "done"}>
  <span class="node {dueClass}"></span>
  <button class="check" onclick={toggle} aria-label="切换完成">
    {#if task.status === "done"}✓{:else}<span class="box"></span>{/if}
  </button>

  <div class="main" onclick={open} role="button" tabindex="0" onkeydown={(e) => e.key === "Enter" && open()}>
    <div class="title">{task.title}</div>
    <div class="meta">
      {#if task.rrule}<span class="badge rrule">↻</span>{/if}
      {#each tags as t}
        <span class="badge tag">@{t.name}</span>
      {/each}
      {#if due != null}
        <span class="badge due {dueClass}">⏰ {fmtDateTime(due)}</span>
      {/if}
    </div>
  </div>

  <div class="actions">
    <button class="act" title="改期" onclick={schedule}>⏰</button>
    <button class="act" title="归档" onclick={archive}>🗄</button>
  </div>
</div>

<style>
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.55rem 0.6rem 0.55rem 1.4rem;
    border-radius: var(--radius);
    transition: background 0.16s, transform 0.16s var(--ease-spring);
  }
  .row:hover {
    background: var(--ink-700);
    transform: translateX(2px);
  }
  .row.selected {
    background: var(--ink-700);
    box-shadow: inset 2px 0 0 var(--amber);
  }
  .node {
    position: absolute;
    left: -1.05rem;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--rule);
    transform: translateX(-50%);
    z-index: 1;
  }
  .node.soon {
    background: var(--amber);
    box-shadow: 0 0 8px var(--amber);
  }
  .node.over {
    background: var(--rose);
    box-shadow: 0 0 8px var(--rose);
    animation: pulse 1.6s infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }
  .check {
    border: none;
    background: transparent;
    padding: 0;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    color: var(--ink-900);
    font-size: 0.9rem;
    font-weight: 700;
  }
  .check .box {
    width: 18px;
    height: 18px;
    border: 1.5px solid var(--rule);
    border-radius: 50%;
  }
  .row:hover .check .box {
    border-color: var(--amber-soft);
  }
  .row.done .check {
    background: var(--sage);
    border-radius: 50%;
  }
  .main {
    flex: 1;
    min-width: 0;
    cursor: pointer;
  }
  .title {
    font-family: var(--font-cjk);
    font-size: 0.98rem;
    color: var(--paper);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row.done .title {
    color: var(--paper-dim);
    text-decoration: line-through;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-top: 0.2rem;
  }
  .badge {
    font-size: 0.72rem;
    font-family: var(--font-mono);
    padding: 0.05rem 0.45rem;
    border-radius: 999px;
    border: 1px solid var(--rule);
    color: var(--paper-dim);
  }
  .badge.tag {
    color: var(--sage);
    border-color: transparent;
    background: rgba(155, 176, 138, 0.12);
  }
  .badge.rrule {
    color: var(--amber);
  }
  .badge.due.soon {
    color: var(--amber);
    border-color: var(--amber-soft);
  }
  .badge.due.over {
    color: var(--rose);
    border-color: var(--rose);
  }
  .actions {
    display: flex;
    gap: 0.25rem;
    opacity: 0;
    transition: opacity 0.16s;
  }
  .row:hover .actions {
    opacity: 1;
  }
  .act {
    border: none;
    background: transparent;
    padding: 0.2rem 0.35rem;
    font-size: 0.9rem;
    color: var(--paper-dim);
  }
  .act:hover {
    color: var(--amber);
  }
</style>
