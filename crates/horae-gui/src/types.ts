export interface ChecklistItem {
  id: string;
  title: string;
  done: boolean;
}

export type TaskStatus =
  | "inbox"
  | "next"
  | "scheduled"
  | "waiting"
  | "someday"
  | "reference"
  | "done";

export interface Task {
  id: string;
  title: string;
  notes: string;
  status: TaskStatus;
  rrule: string | null;
  created_at: number;
  clarified_at: number | null;
  due_at: number | null;
  scheduled_start_at: number | null;
  scheduled_end_at: number | null;
  completed_at: number | null;
  archived_at: number | null;
  archive_reason: string | null;
  updated_at: number;
  delegated_to: string | null;
  checklist: ChecklistItem[];
}

export interface Tag {
  id: number;
  name: string;
  category: string;
  is_system: boolean;
  color: string | null;
  icon: string | null;
  description: string | null;
  created_at: number;
}

export type NotificationEvent =
  | { InOneHour: { title: string } }
  | { InTenMins: { title: string } }
  | { Now: { id: string; title: string } };

export interface Profiles {
  default: string;
  names: string[];
}

export interface TaskEvent {
  id: number;
  task_id: string;
  event_type: string;
  from_status: string | null;
  to_status: string | null;
  at: number;
  meta: string | null;
}

export interface TaskDetail {
  task: Task;
  events: TaskEvent[];
}
