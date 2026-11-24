import {
  BREAK_MINUTES,
  SNOOZE_MINUTES,
  type PopupContent,
  type ReminderKind,
  type Settings,
  type Snapshot,
} from "./types";
import { formatClockTime, formatDuration } from "./format";
import { createTranslator, type Translator } from "./i18n";

const ENGLISH = createTranslator("en");

export type Phase =
  | "running"
  | "paused"
  | "snoozed"
  | "awaiting"
  | "break"
  | "disabled";

export interface MainWindowCopy {
  /** Caption shown above the countdown inside the ring. */
  label: string;
  /** Already-formatted MM:SS (or `--:--` when reminders are off). */
  time: string;
  /** Two short lines of supporting copy under the ring. */
  support: string;
  phase: Phase;
  /** Whether the ring should be dimmed. */
  muted: boolean;
}

export function remindersDisabled(settings: Settings): boolean {
  return !settings.eyesEnabled && !settings.movementEnabled;
}

/**
 * Maps the authoritative scheduler snapshot onto what the main window shows.
 *
 * Kept as a pure function (no React, no IPC) so the state-to-copy mapping is
 * unit-testable and the component stays declarative.
 */
export function describeMainWindow(
  settings: Settings,
  snapshot: Snapshot,
  t: Translator = ENGLISH,
): MainWindowCopy {
  if (remindersDisabled(settings)) {
    return {
      label: t("main.off.label"),
      time: "--:--",
      support: t("main.off.support"),
      phase: "disabled",
      muted: true,
    };
  }

  if (snapshot.state === "awaiting") {
    const kind = snapshot.pending?.kind ?? snapshot.nextKind;
    const which = kind === "movement" ? "movement" : "eyes";
    return {
      label: t(`main.awaiting.${which}.label`),
      time: formatDuration(0),
      support: t(`main.awaiting.${which}.support`),
      phase: "awaiting",
      muted: false,
    };
  }

  if (snapshot.state === "break") {
    const which = snapshot.pending?.kind === "movement" ? "movement" : "eyes";
    return {
      label: t(`main.break.${which}.label`),
      time: formatDuration(snapshot.remainingMs),
      support: t(`main.break.${which}.support`),
      phase: "break",
      muted: false,
    };
  }

  if (snapshot.state === "paused") {
    return {
      label: t("main.paused.label"),
      time: formatDuration(snapshot.remainingMs),
      support: t("main.paused.support"),
      phase: "paused",
      muted: true,
    };
  }

  if (snapshot.state === "snoozed") {
    const at = formatClockTime(snapshot.nextAtMs, t);
    return {
      label: t("main.snoozed.label"),
      time: formatDuration(snapshot.snoozeRemainingMs ?? snapshot.remainingMs),
      support: at ? t("main.snoozed.supportAt", { time: at }) : t("main.snoozed.supportSoon"),
      phase: "snoozed",
      muted: true,
    };
  }

  return {
    label: runningLabel(snapshot.nextKind, t),
    time: formatDuration(snapshot.remainingMs),
    support: t("main.running.support"),
    phase: "running",
    muted: false,
  };
}

function runningLabel(kind: ReminderKind | null, t: Translator): string {
  switch (kind) {
    case "movement":
      return t("main.next.movement");
    case "eyes":
      return t("main.next.eyes");
    default:
      return t("main.next.any");
  }
}

/** Label for the single primary action in the main window. */
export function primaryActionLabel(phase: Phase, t: Translator = ENGLISH): string {
  switch (phase) {
    case "awaiting":
      return breakActionLabel(t);
    case "snoozed":
      return t("action.resumeNow");
    case "paused":
      return t("action.resume");
    default:
      return t("action.pause");
  }
}

/** Accessible name of the primary action, naming what it acts on. */
export function primaryActionAriaLabel(phase: Phase, t: Translator = ENGLISH): string {
  switch (phase) {
    case "snoozed":
      return t("action.resumeNowReminders");
    case "paused":
      return t("action.resumeReminders");
    default:
      return t("action.pauseReminders");
  }
}

/**
 * `intervalMinutes` is stored in settings but must be read back from the
 * snapshot, which is what the scheduler actually runs on.
 */
export function effectiveIntervalMinutes(
  settings: Settings,
  snapshot: Snapshot,
): number {
  return snapshot.intervalMinutes || settings.intervalMinutes;
}

export function breakActionLabel(t: Translator = ENGLISH): string {
  return t("action.takeBreak", { count: BREAK_MINUTES });
}

export function snoozeActionLabel(t: Translator = ENGLISH): string {
  return t("action.snooze", { count: SNOOZE_MINUTES });
}

export interface PopupCopy {
  title: string;
  body: string;
  kind: ReminderKind;
  /** Whether the popup offers the break and the snooze. */
  actionable: boolean;
  /** Small caption above the title. */
  eyebrow: string;
}

/**
 * Maps popup content onto what the popup shows. Pure, like
 * `describeMainWindow`, so the copy is unit-testable.
 */
export function describePopup(content: PopupContent, t: Translator = ENGLISH): PopupCopy {
  switch (content.type) {
    case "reminder":
      return {
        ...reminderCopy(content.reminder.kind, t),
        kind: content.reminder.kind,
        actionable: true,
        eyebrow: t("popup.eyebrow"),
      };
    case "test":
      return {
        ...reminderCopy(content.kind, t),
        kind: content.kind,
        actionable: false,
        eyebrow: t("popup.preview"),
      };
    case "breakOver":
      return {
        title: t("reminder.breakOver.title"),
        body: t("reminder.breakOver.body"),
        kind: content.kind,
        actionable: false,
        eyebrow: t("popup.eyebrow"),
      };
  }
}

function reminderCopy(kind: ReminderKind, t: Translator): { title: string; body: string } {
  const which = kind === "movement" ? "movement" : "eyes";
  return {
    title: t(`reminder.${which}.title`),
    body: t(`reminder.${which}.body`),
  };
}
