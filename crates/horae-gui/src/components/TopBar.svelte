<script lang="ts">
  import { store } from "../store.svelte";

  let input = $state("");
  let busy = $state(false);
  let hint = $state(false);

  async function submit() {
    const v = input.trim();
    if (!v || busy) return;
    busy = true;
    try {
      await store.doCapture(v);
      input = "";
    } catch {
      /* 错误已写入 store.error，保留输入以便修改 */
    } finally {
      busy = false;
    }
  }
</script>

<header class="topbar">
  <div class="brand">
    <span class="wordmark">⌁ horae</span>
    <span class="tagline faint">计时匠的书桌</span>
  </div>

  <div class="capture">
    <input
      id="quick-add"
      placeholder="快速捕获：买牛奶 ~18:00 @home *daily ！回车录入"
      bind:value={input}
      onfocus={() => (hint = true)}
      onblur={() => (hint = false)}
      onkeydown={(e) => {
        if (e.key === "Enter") submit();
      }}
    />
    <button onclick={submit} disabled={busy || !input.trim()}>添加</button>
    {#if hint}
      <div class="chips">
        <span class="chip">@标签</span>
        <span class="chip">~时间</span>
        <span class="chip">*循环</span>
        <span class="chip">!p1/p2/p3</span>
        <span class="faint">Ctrl/⌘ + N 聚焦</span>
      </div>
    {/if}
  </div>

  <div class="profile">
    {#if store.profiles}
      <span class="faint">档案</span>
      <span class="amber">{store.profiles.default}</span>
    {/if}
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 1.2rem;
    padding: 0.7rem 1.1rem;
    border-bottom: 1px solid var(--rule);
    background: linear-gradient(180deg, rgba(0, 0, 0, 0.25), transparent);
  }
  .brand {
    display: flex;
    flex-direction: column;
    line-height: 1.1;
  }
  .brand .wordmark {
    font-size: 1.35rem;
    color: var(--paper);
  }
  .brand .tagline {
    font-family: var(--font-cjk);
    font-size: 0.7rem;
    letter-spacing: 0.15em;
  }
  .capture {
    position: relative;
    flex: 1;
    display: flex;
    gap: 0.5rem;
    max-width: 720px;
  }
  .capture input {
    flex: 1;
    font-family: var(--font-cjk);
    font-size: 0.95rem;
    padding: 0.5rem 0.75rem;
  }
  .capture button {
    padding: 0.5rem 1rem;
    border-color: var(--amber-soft);
    color: var(--amber);
  }
  .capture button:disabled {
    opacity: 0.4;
    border-color: var(--rule);
    color: var(--paper-dim);
  }
  .chips {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    display: flex;
    gap: 0.5rem;
    align-items: center;
    font-size: 0.75rem;
    animation: drop 0.25s var(--ease-spring);
  }
  .chip {
    font-family: var(--font-mono);
    background: var(--ink-700);
    border: 1px solid var(--rule);
    border-radius: 999px;
    padding: 0.15rem 0.6rem;
    color: var(--amber);
  }
  .profile {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
  }
  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
