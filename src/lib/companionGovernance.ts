export interface GovernedSuggestion {
  kind: string;
  priority: number;
  text: string;
}

export interface DoNotDisturbWindow {
  label: string;
  startMinute: number;
  endMinute: number;
  contains: (minuteOfDay: number) => boolean;
}

export interface DoNotDisturbResult<T extends GovernedSuggestion> {
  active: boolean;
  suppressedCount: number;
  suggestions: T[];
  reason: string;
}

function parseClock(hours: string, minutes: string) {
  const hour = Number(hours);
  const minute = Number(minutes);
  if (!Number.isInteger(hour) || !Number.isInteger(minute) || hour > 23 || minute > 59) return null;
  return hour * 60 + minute;
}

export function parseDoNotDisturbWindow(value: string): DoNotDisturbWindow | null {
  const normalized = value.trim().replaceAll("：", ":");
  if (!normalized) return null;
  const match = /^(\d{1,2}):(\d{2})\s*(?:-|~|–|—|至)\s*(\d{1,2}):(\d{2})$/.exec(normalized);
  if (!match) return null;
  const startMinute = parseClock(match[1], match[2]);
  const endMinute = parseClock(match[3], match[4]);
  if (startMinute === null || endMinute === null) return null;
  const label = `${match[1].padStart(2, "0")}:${match[2]}-${match[3].padStart(2, "0")}:${match[4]}`;
  return {
    label,
    startMinute,
    endMinute,
    contains(minuteOfDay: number) {
      const minute = ((Math.floor(minuteOfDay) % 1440) + 1440) % 1440;
      if (startMinute === endMinute) return true;
      if (startMinute < endMinute) return minute >= startMinute && minute < endMinute;
      return minute >= startMinute || minute < endMinute;
    },
  };
}

function proactiveCandidate(kind: string) {
  return kind === "check_in" || kind.startsWith("reminder_");
}

export function applyDoNotDisturb<T extends GovernedSuggestion>(
  suggestions: T[],
  value: string,
  now = new Date(),
): DoNotDisturbResult<T> {
  const configured = value.trim();
  if (!configured) {
    return { active: false, suppressedCount: 0, suggestions: [...suggestions], reason: "" };
  }
  const window = parseDoNotDisturbWindow(configured);
  if (!window) {
    const warning = {
      kind: "reminder_policy_invalid",
      priority: 1,
      text: `免打扰时段“${configured}”无法解析，请使用 HH:mm-HH:mm。`,
    } as T;
    return {
      active: false,
      suppressedCount: 0,
      suggestions: [...suggestions, warning],
      reason: warning.text,
    };
  }

  const active = window.contains(now.getHours() * 60 + now.getMinutes());
  if (!active) {
    return { active: false, suppressedCount: 0, suggestions: [...suggestions], reason: "" };
  }

  const kept = suggestions.filter((item) => !proactiveCandidate(item.kind) || item.priority >= 3);
  const suppressedCount = suggestions.length - kept.length;
  const reason = `免打扰 ${window.label} 生效中，已抑制 ${suppressedCount} 条主动提醒。`;
  kept.push({ kind: "reminder_suppressed", priority: 1, text: reason } as T);
  return { active: true, suppressedCount, suggestions: kept, reason };
}
