import { sendNotification } from "@tauri-apps/plugin-notification";
import { tickNotifications } from "./lib";
import type { NotificationEvent } from "./types";

// 系统通知封装（番茄钟阶段、到期提醒共用）。
export async function notifySystem(title: string, body: string) {
  try {
    sendNotification({ title, body });
  } catch {
    /* 无通知权限时静默 */
  }
}

function present(e: NotificationEvent) {
  if ("Now" in e) notifySystem("⏰ 时间到", e.Now.title);
  else if ("InTenMins" in e) notifySystem("🟡 10 分钟后", e.InTenMins.title);
  else if ("InOneHour" in e) notifySystem("🔔 1 小时后", e.InOneHour.title);
}

// 每 30s 轮询一次到期提醒；首次立即执行。返回 timer 句柄。
export function startNotificationLoop(): number {
  const tick = async () => {
    try {
      const events = await tickNotifications();
      for (const e of events) present(e);
    } catch {
      /* 忽略轮询错误 */
    }
  };
  tick();
  return window.setInterval(tick, 30000);
}
