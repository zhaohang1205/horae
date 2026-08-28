import { invoke } from "@tauri-apps/api/core";
import type {
  Task,
  Tag,
  TaskDetail,
  NotificationEvent,
  Profiles,
} from "./types";

export const listTasks = (view: string) =>
  invoke<Task[]>("list_tasks", { view });

export const capture = (input: string) =>
  invoke<Task>("capture", { input });

export const transition = (id: string, status: string) =>
  invoke<Task>("transition", { id, status });

export const setDue = (id: string, dueMs: number | null) =>
  invoke<Task>("set_due", { id, dueMs });

export const schedule = (id: string, startMs: number, endMs: number | null) =>
  invoke<Task>("schedule", { id, startMs, endMs });

export const archive = (id: string) => invoke<Task>("archive", { id });
export const unarchive = (id: string) => invoke<Task>("unarchive", { id });
export const purge = (id: string) => invoke<Task>("purge", { id });

export const detail = (id: string) =>
  invoke<TaskDetail>("detail", { id });

export const rename = (id: string, title: string) =>
  invoke<Task>("rename", { id, title });

export const updateNotes = (id: string, notes: string) =>
  invoke<Task>("update_notes", { id, notes });

export const toggleChecklistItem = (id: string, itemId: string) =>
  invoke<string | null>("toggle_checklist_item", { id, itemId });

export const listTags = () => invoke<Tag[]>("list_tags");
export const createTag = (name: string) =>
  invoke<number>("create_tag", { name });
export const deleteTag = (name: string) =>
  invoke<void>("delete_tag", { name });
export const addTagToTask = (taskId: string, tagName: string) =>
  invoke<void>("add_tag_to_task", { taskId, tagName });
export const removeTagFromTask = (taskId: string, tagName: string) =>
  invoke<void>("remove_tag_from_task", { taskId, tagName });
export const getTaskTags = (taskId: string) =>
  invoke<Tag[]>("get_task_tags", { taskId });

export const pomoState = () => invoke<unknown>("pomo_state");
export const startPomo = (id: string) =>
  invoke<void>("start_pomo", { id });

export const getSetting = (key: string) =>
  invoke<string | null>("get_setting", { key });
export const setSetting = (key: string, value: string) =>
  invoke<void>("set_setting", { key, value });

export const tickNotifications = () =>
  invoke<NotificationEvent[]>("tick_notifications");

export const listProfiles = () => invoke<Profiles>("list_profiles");
