// 时间显示工具：core 返回 UTC-ms，这里按本地时区展示/回填。
const p = (n: number) => String(n).padStart(2, "0");

export function fmtDateTime(ms: number): string {
  const d = new Date(ms);
  return `${p(d.getMonth() + 1)}/${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function fmtDate(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function fmtRelative(ms: number): string {
  const diff = ms - Date.now();
  const abs = Math.abs(diff);
  const m = Math.round(abs / 60000);
  const h = Math.round(abs / 3600000);
  const d = Math.round(abs / 86400000);
  const tag = diff >= 0 ? "后" : "前";
  if (abs < 60000) return "刚刚";
  if (m < 60) return `${m} 分钟${tag}`;
  if (h < 24) return `${h} 小时${tag}`;
  return `${d} 天${tag}`;
}

// 生成 <input type="datetime-local"> 的 value（本地时区，无 Z）
export function toLocalInput(ms: number): string {
  const d = new Date(ms - new Date(ms).getTimezoneOffset() * 60000);
  return d.toISOString().slice(0, 16);
}

export function fromLocalInput(s: string): number {
  return new Date(s).getTime();
}
