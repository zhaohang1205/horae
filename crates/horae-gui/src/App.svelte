<script lang="ts">
  import { listTasks, capture, transition } from "./lib";
  import type { Task } from "./types";

  let tasks = $state<Task[]>([]);
  let input = $state("");
  let error = $state("");

  async function refresh() {
    try {
      tasks = await listTasks("today");
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function doCapture() {
    const v = input.trim();
    if (!v) return;
    try {
      await capture(v);
      input = "";
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function toggle(task: Task) {
    try {
      const next = task.status === "done" ? "next" : "done";
      await transition(task.id, next);
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    refresh();
  });
</script>

<main>
  <h1>horae</h1>
  <div class="capture">
    <input
      placeholder="快速捕获，例如：买牛奶 ~18:00 @home"
      bind:value={input}
      onkeydown={(e) => {
        if (e.key === "Enter") doCapture();
      }}
    />
    <button onclick={doCapture}>添加</button>
  </div>
  {#if error}
    <p class="error">{error}</p>
  {/if}
  <ul>
    {#each tasks as t (t.id)}
      <li>
        <input
          type="checkbox"
          checked={t.status === "done"}
          onchange={() => toggle(t)}
        />
        <span class:done={t.status === "done"}>{t.title}</span>
      </li>
    {/each}
  </ul>
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    padding: 1rem;
    max-width: 720px;
    margin: 0 auto;
  }
  .capture {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }
  .capture input {
    flex: 1;
    padding: 0.5rem;
  }
  ul {
    list-style: none;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.25rem 0;
  }
  .done {
    text-decoration: line-through;
    color: #888;
  }
  .error {
    color: #f38ba8;
  }
</style>
