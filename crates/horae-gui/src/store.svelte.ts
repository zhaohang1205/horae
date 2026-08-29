import {
  listTasks,
  capture,
  transition,
  setDue,
  schedule,
  archive,
  unarchive,
  purge,
  detail,
  rename,
  updateNotes,
  toggleChecklistItem,
  addChecklistItem,
  deleteChecklistItem,
  listTags,
  addTagToTask,
  removeTagFromTask,
  getTaskTags,
  pomoState,
  startPomo,
  pomoComplete,
  pomoStop,
  listProfiles,
} from "./lib";
import type { Task, TaskDetail, PomoState, Tag, Profiles, TaskStatus } from "./types";

export type ViewName =
  | "today"
  | "inbox"
  | "next"
  | "scheduled"
  | "waiting"
  | "someday"
  | "reference"
  | "archived"
  | "all";

export const VIEW_LABELS: Record<ViewName, string> = {
  today: "今天",
  inbox: "收集箱",
  next: "下一步",
  scheduled: "已排期",
  waiting: "等待中",
  someday: "将来也许",
  reference: "参考",
  archived: "归档",
  all: "全部",
};

class Store {
  view = $state<ViewName>("today");
  tasks = $state<Task[]>([]);
  selectedId = $state<string | null>(null);
  detailData = $state<TaskDetail | null>(null);
  tags = $state<Tag[]>([]);
  pomo = $state<PomoState | null>(null);
  profiles = $state<Profiles | null>(null);
  error = $state("");
  reviewOpen = $state(false);
  reviewStep = $state(0);

  async refresh() {
    try {
      this.tasks = await listTasks(this.view);
      this.error = "";
    } catch (e) {
      this.error = String(e);
    }
  }

  async select(id: string) {
    this.selectedId = id;
    try {
      this.detailData = await detail(id);
      this.error = "";
    } catch (e) {
      this.error = String(e);
    }
  }

  closeDetail() {
    this.selectedId = null;
    this.detailData = null;
  }

  async loadTags() {
    try {
      this.tags = await listTags();
    } catch {
      /* 忽略：标签加载失败不阻塞主界面 */
    }
  }

  async loadProfiles() {
    try {
      this.profiles = await listProfiles();
    } catch {
      /* 忽略 */
    }
  }

  async loadPomo() {
    try {
      this.pomo = await pomoState();
    } catch {
      /* 忽略 */
    }
  }

  async doCapture(raw: string) {
    const v = raw.trim();
    if (!v) return;
    try {
      await capture(v);
      await this.refresh();
      this.error = "";
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  async setStatus(id: string, status: TaskStatus) {
    await transition(id, status);
    await this.refresh();
    if (this.selectedId === id) await this.select(id);
  }

  async doToggle(id: string, current: TaskStatus) {
    const next: TaskStatus = current === "done" ? "next" : "done";
    await this.setStatus(id, next);
  }

  async doArchive(id: string) {
    await archive(id);
    if (this.selectedId === id) this.closeDetail();
    await this.refresh();
  }

  async doUnarchive(id: string) {
    await unarchive(id);
    await this.refresh();
    if (this.selectedId === id) await this.select(id);
  }

  async doPurge(id: string) {
    await purge(id);
    if (this.selectedId === id) this.closeDetail();
    await this.refresh();
  }

  async doRename(id: string, title: string) {
    await rename(id, title);
    await this.refresh();
    await this.select(id);
  }

  async doUpdateNotes(id: string, notes: string) {
    await updateNotes(id, notes);
    await this.select(id);
  }

  async doToggleChecklist(id: string, itemId: string) {
    await toggleChecklistItem(id, itemId);
    await this.select(id);
  }

  async doAddChecklistItem(id: string, title: string) {
    await addChecklistItem(id, title);
    await this.select(id);
  }

  async doDeleteChecklistItem(id: string, itemId: string) {
    await deleteChecklistItem(id, itemId);
    await this.select(id);
  }

  async doSetDue(id: string, dueMs: number | null) {
    await setDue(id, dueMs);
    await this.select(id);
  }

  async doSchedule(id: string, startMs: number, endMs: number | null) {
    await schedule(id, startMs, endMs);
    await this.select(id);
  }

  async doAddTag(id: string, name: string) {
    await addTagToTask(id, name);
    await this.select(id);
  }

  async doRemoveTag(id: string, name: string) {
    await removeTagFromTask(id, name);
    await this.select(id);
  }

  async doStartPomo(id: string) {
    this.pomo = await startPomo(id);
  }

  async doCompletePomo() {
    this.pomo = await pomoComplete();
    return this.pomo;
  }

  async doStopPomo() {
    this.pomo = await pomoStop();
  }
}

export const store = new Store();
