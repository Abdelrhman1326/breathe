import { createTranslator, type Translator } from "./i18n";
import type { ReminderKind, ScheduleState } from "./types";

const ENGLISH = createTranslator("en");

/** Formats a millisecond duration as MM:SS, rounding up so the last second is visible. */
export function formatDuration(ms: number): string {
  const safe = Number.isFinite(ms) ? ms : 0;
  const totalSeconds = Math.max(0, Math.ceil(safe / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

/** Formats an epoch-millisecond deadline as a short, locale-aware clock time. */
export function formatClockTime(
  epochMs: number | null,
  t: Translator = ENGLISH,
): string | null {
  if (epochMs === null || !Number.isFinite(epochMs)) return null;
  return new Intl.DateTimeFormat(t.locale, {
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(epochMs));
}

export function formatMinutes(minutes: number, t: Translator = ENGLISH): string {
  return t.plural("unit.minutes", minutes);
}

export function intervalLabel(minutes: number, t: Translator = ENGLISH): string {
  return t("interval.every", { duration: formatMinutes(minutes, t) });
}

export function reminderLabel(kind: ReminderKind | null, t: Translator = ENGLISH): string {
  switch (kind) {
    case "eyes":
      return t("kind.eyes");
    case "movement":
      return t("kind.movement");
    default:
      return t("kind.none");
  }
}

export function scheduleLabel(state: ScheduleState, t: Translator = ENGLISH): string {
  return t(`state.${state}`);
}

/** Clamps a fraction into 0..1; used for the progress ring. */
export function clampFraction(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}
