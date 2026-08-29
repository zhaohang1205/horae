<script lang="ts">
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import { store } from "../store.svelte";
  import { getTaskTags } from "../lib";
  import type { Tag, TaskStatus } from "../types";
  import { fmtDateTime, toLocalInput, fromLocalInput } from "../format";

  const d = $derived(store.detailData!);
  const task = $derived(d.task);

  let tags = $state<Tag[]>([]);
  let notes = $state(task.notes);
  let newItem = $state("");
  let newTag = $state("");
  let notesTimer: number | undefined;

  const STATUSES: TaskStatus[] = [
    "inbox",
    "next",
    "scheduled",
    "waiting",
    "someday",
    "reference",
    "done",
  ];
  const STATUS_LABEL: Record<TaskStatus, string> = {
    inbox: "收集箱",
    next: "下一步",
    scheduled: "已排期",
    waiting: "等待中",
    someday: "将来也许",
    reference: "参考",
    done: "已完成",
  };

  async function loadTags() {
    try {
      tags = await getTaskTags(task.id);
    } catch {
      /* 忽略 */
    }
  }

  // detail 变化时重置本地编辑态
  $effect(() => {
    if (store.detailData) {
      notes = store.detailData.task.notes;
      loadTags();
    }
  });

  function onNotes() {
    clearTimeout(notesTimer);
    notesTimer = window.setTimeout(() => store.doUpdateNotes(task.id, notes), 500);
  }

  async function onDue(e: Event) {
    const v = (e.target as HTMLInputElement).value;
    await store.doSetDue(task.id, v ? fromLocalInput(v) : null);
  }
  async function onSchedule(e: Event) {
    const v = (e.target as HTMLInputElement).value;
    await store.doSchedule(task.id, v ? fromLocalInput(v) : Date.now(), null);
  }

  async function addTag() {
    const n = newTag.trim().replace(/^@/, "");
    if (!n) return;
    await store.doAddTag(task.id, n);
    newTag = "";
    await loadTags();
  }

  async function startPomo() {
    await store.doStartPomo(task.id);
  }
</script>

<div
  class="scrim"
  role="button"
  tabindex="-1"
  aria-label="关闭详情"
  onclick={() => store.closeDetail()}
  onkeydown={(e) => e.key === "Escape" && store.closeDetail()}
></div>
<aside class="drawer" transition:fly={{ x: 420, duration: 320, opacity: 0.4 }}>
  <div class="dh">
    <span class="kicker faint">任务详情</span>
    <button class="x" onclick={() => store.closeDetail()}>✕</button>
  </div>

  <input class="title" value={task.title} onblur={(e) => store.doRename(task.id, e.currentTarget.value)} />

  <div class="grid">
    <label class="fld">
      <span class="faint">状态</span>
      <select
        value={task.status}
        onchange={(e) => store.setStatus(task.id, e.currentTarget.value as TaskStatus)}
      >
        {#each STATUSES as s}
          <option value={s} selected={s === task.status}>{STATUS_LABEL[s]}</option>
        {/each}
      </select>
    </label>
    <label class="fld">
      <span class="faint">到期</span>
      <input type="datetime-local" value={task.due_at ? toLocalInput(task.due_at) : ""} onchange={onDue} />
    </label>
    <label class="fld">
      <span class="faint">排期开始</span>
      <input
        type="datetime-local"
        value={task.scheduled_start_at ? toLocalInput(task.scheduled_start_at) : ""}
        onchange={onSchedule}
      />
    </label>
    {#if task.rrule}
      <label class="fld">
        <span class="faint">循环</span>
        <input value={task.rrule} readonly />
      </label>
    {/if}
  </div>

  <section class="block">
    <h3>清单</h3>
    <ul class="checklist">
      {#each task.checklist as item (item.id)}
        <li>
          <button
            class="ci"
            class:done={item.done}
            onclick={() => store.doToggleChecklist(task.id, item.id)}
          >{item.done ? "✓" : ""}</button>
          <span class:done={item.done}>{item.title}</span>
          <button class="del" onclick={() => store.doDeleteChecklistItem(task.id, item.id)}>✕</button>
        </li>
      {/each}
    </ul>
    <div class="addrow">
      <input placeholder="新增清单项，回车" bind:value={newItem} onkeydown={(e) => {
        if (e.key === "Enter" && newItem.trim()) {
          store.doAddChecklistItem(task.id, newItem.trim());
          newItem = "";
        }
      }} />
    </div>
  </section>

  <section class="block">
    <h3>笔记</h3>
    <textarea rows="4" bind:value={notes} oninput={onNotes} placeholder="记录上下文、链接、思路…"></textarea>
  </section>

  <section class="block">
    <h3>标签</h3>
    <div class="tags">
      {#each tags as t}
        <span class="tag">@{t.name}<button class="xt" onclick={() => store.doRemoveTag(task.id, t.name)}>✕</button></span>
      {/each}
    </div>
    <div class="addrow">
      <input
        placeholder="加标签，回车"
        bind:value={newTag}
        list="tag-options"
        onkeydown={(e) => e.key === "Enter" && addTag()}
      />
      <datalist id="tag-options">
        {#each store.tags as t}<option value={t.name} />{/each}
      </datalist>
    </div>
  </section>

  {#if d.events.length}
    <section class="block">
      <h3>时间线</h3>
      <ul class="events">
        {#each d.events.slice().reverse() as ev}
          <li><span class="faint">{fmtDateTime(ev.at)}</span> · {ev.event_type}</li>
        {/each}
      </ul>
    </section>
  {/if}

  <div class="actions">
    <button class="pomo" onclick={startPomo}>▶ 开始番茄</button>
    {#if task.archived_at}
      <button onclick={() => store.doUnarchive(task.id)}>↩ 取回</button>
      <button class="danger" onclick={() => store.doPurge(task.id)}>🗑 彻底删除</button>
    {:else}
      <button onclick={() => store.doArchive(task.id)}>🗄 归档</button>
    {/if}
  </div>
</aside>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 20;
    animation: fade 0.25s ease;
  }
  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    height: 100vh;
    width: min(440px, 92vw);
    z-index: 21;
    background: var(--ink-800);
    border-left: 1px solid var(--rule);
    box-shadow: -20px 0 50px rgba(0, 0, 0, 0.45);
    padding: 1.1rem 1.2rem 2rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }
  .dh {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .kicker {
    font-family: var(--font-cjk);
    letter-spacing: 0.2em;
    font-size: 0.75rem;
  }
  .x {
    border: none;
    background: transparent;
    color: var(--paper-dim);
    font-size: 1rem;
  }
  .title {
    font-family: var(--font-display);
    font-size: 1.35rem;
    font-weight: 600;
    color: var(--paper);
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--amber-soft);
    border-radius: 0;
    padding: 0.3rem 0.1rem;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }
  .fld {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
  }
  .block h3 {
    margin: 0 0 0.4rem;
    font-family: var(--font-cjk);
    font-size: 0.85rem;
    color: var(--paper-dim);
    letter-spacing: 0.1em;
  }
  .checklist {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .checklist li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .checklist .ci {
    width: 20px;
    height: 20px;
    border: 1.5px solid var(--rule);
    border-radius: 5px;
    background: transparent;
    color: var(--ink-900);
    font-weight: 700;
    display: grid;
    place-items: center;
  }
  .checklist .ci.done {
    background: var(--sage);
    border-color: var(--sage);
  }
  .checklist .done {
    color: var(--paper-dim);
    text-decoration: line-through;
  }
  .checklist .del {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--paper-faint);
  }
  .addrow input {
    width: 100%;
  }
  textarea {
    width: 100%;
    resize: vertical;
    font-family: var(--font-cjk);
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-family: var(--font-mono);
    font-size: 0.8rem;
    color: var(--sage);
    background: rgba(155, 176, 138, 0.12);
    border-radius: 999px;
    padding: 0.15rem 0.55rem;
  }
  .tag .xt {
    border: none;
    background: transparent;
    color: var(--paper-faint);
    font-size: 0.7rem;
    padding: 0;
  }
  .events {
    list-style: none;
    margin: 0;
    padding: 0;
    font-family: var(--font-mono);
    font-size: 0.78rem;
    color: var(--paper-dim);
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: auto;
    padding-top: 0.8rem;
    border-top: 1px solid var(--rule);
  }
  .actions .pomo {
    border-color: var(--amber-soft);
    color: var(--amber);
  }
  .actions .danger {
    color: var(--rose);
    border-color: var(--rose);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>
