<script lang="ts">
  import { onMount } from "svelte";
  import { store } from "./store.svelte";
  import { startNotificationLoop } from "./notifications";
  import TopBar from "./components/TopBar.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import TaskList from "./components/TaskList.svelte";
  import TaskDetail from "./components/TaskDetail.svelte";
  import PomoWidget from "./components/PomoWidget.svelte";
  import ReviewModal from "./components/ReviewModal.svelte";

  onMount(async () => {
    await Promise.all([
      store.refresh(),
      store.loadTags(),
      store.loadProfiles(),
      store.loadPomo(),
    ]);
    const timer = startNotificationLoop();
    return () => clearInterval(timer);
  });

  function onKeydown(e: KeyboardEvent) {
    // Ctrl/Cmd+N 聚焦快速捕获框（鼠标用户也能一键落入录入）
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
      e.preventDefault();
      const el = document.getElementById("quick-add") as HTMLInputElement | null;
      el?.focus();
    }
    if (e.key === "Escape" && store.selectedId) store.closeDetail();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <TopBar />
  <div class="body">
    <Sidebar />
    <TaskList />
  </div>
  <PomoWidget />
  {#if store.detailData}
    <TaskDetail />
  {/if}
  {#if store.reviewOpen}
    <ReviewModal />
  {/if}
</div>

<style>
  .app {
    position: relative;
    z-index: 1;
    height: 100vh;
    display: grid;
    grid-template-rows: auto 1fr auto;
  }
  .body {
    display: grid;
    grid-template-columns: 248px 1fr;
    min-height: 0;
    overflow: hidden;
  }
</style>
