import {
  BREAK_MINUTES,
  SNOOZE_MINUTES,
  type PopupContent,
  type ReminderKind,
  type Settings,
  type Snapshot,
} from "./types";
import { formatClockTime, formatDuration } from "./format";

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
): MainWindowCopy {
  if (remindersDisabled(settings)) {
    return {
      label: "Reminders are off",
      time: "--:--",
      support: "Turn on eye rest or movement reminders in Settings.",
      phase: "disabled",
      muted: true,
    };
  }

  if (snapshot.state === "awaiting") {
    const kind = snapshot.pending?.kind ?? snapshot.nextKind;
    return {
      label: kind === "movement" ? "Time to move a little" : "Time to rest your eyes",
      time: formatDuration(0),
      support:
        kind === "movement"
          ? "Stand up and stretch. Take a short break, or snooze for a while."
          : "Look into the distance. Take a short break, or snooze for a while.",
      phase: "awaiting",
      muted: false,
    };
  }

  if (snapshot.state === "break") {
    const kind = snapshot.pending?.kind ?? null;
    return {
      label: kind === "movement" ? "Movement break" : "Eye break",
      time: formatDuration(snapshot.remainingMs),
      support:
        kind === "movement"
          ? "Stretch or walk around. The next countdown starts after your break."
          : "Let your eyes rest on something far away. The next countdown starts after your break.",
      phase: "break",
      muted: false,
    };
  }

  if (snapshot.state === "paused") {
    return {
      label: "Paused",
      time: formatDuration(snapshot.remainingMs),
      support: "Reminders are paused. You can resume them anytime.",
      phase: "paused",
      muted: true,
    };
  }

  if (snapshot.state === "snoozed") {
    return {
      label: "Snoozed",
      time: formatDuration(snapshot.snoozeRemainingMs ?? snapshot.remainingMs),
      support: snoozeSupport(snapshot.nextAtMs),
      phase: "snoozed",
      muted: true,
    };
  }

  return {
    label: runningLabel(snapshot.nextKind),
    time: formatDuration(snapshot.remainingMs),
    support: "A short break now helps you focus better later.",
    phase: "running",
    muted: false,
  };
}

function runningLabel(kind: ReminderKind | null): string {
  switch (kind) {
    case "movement":
      return "Next movement break in";
    case "eyes":
      return "Next eye break in";
    default:
      return "Next break in";
  }
}

function snoozeSupport(nextAtMs: number | null): string {
  const at = formatClockTime(nextAtMs);
  return at ? `Next reminder at ${at}.` : "The next reminder is scheduled shortly.";
}

/** Label for the single primary action in the main window. */
export function primaryActionLabel(phase: Phase): string {
  switch (phase) {
    case "awaiting":
      return breakActionLabel();
    case "snoozed":
      return "Resume now";
    case "paused":
      return "Resume";
    default:
      return "Pause";
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

export function breakActionLabel(): string {
  return `Take a ${BREAK_MINUTES}-minute break`;
}

export function snoozeActionLabel(): string {
  return `Snooze ${SNOOZE_MINUTES} minutes`;
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
export function describePopup(content: PopupContent): PopupCopy {
  switch (content.type) {
    case "reminder":
      return {
        ...reminderCopy(content.reminder.kind),
        kind: content.reminder.kind,
        actionable: true,
        eyebrow: "Breathe",
      };
    case "test":
      return {
        ...reminderCopy(content.kind),
        kind: content.kind,
        actionable: false,
        eyebrow: "Preview",
      };
    case "breakOver":
      return {
        title: "Break's over",
        body: "Welcome back. Your next reminder countdown has started.",
        kind: content.kind,
        actionable: false,
        eyebrow: "Breathe",
      };
  }
}

function reminderCopy(kind: ReminderKind): { title: string; body: string } {
  return kind === "movement"
    ? {
        title: "Time to move a little",
        body: "Stand up, stretch, and give your body a chance to move.",
      }
    : {
        title: "Time to rest your eyes",
        body: "Look away from the screen and let your eyes rest on something far away.",
      };
}
