import {
  BREAK_MINUTES,
  SNOOZE_MINUTES,
  type PopupContent,
  type ReminderKind,
  type Settings,
  type Snapshot,
} from "./types";
import { formatClockTime, formatDuration } from "./format";
import { createTranslator, type MessageKey, type Translator } from "./i18n";

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
  /** True when `support` is a classical phrase (typeset in Naskh). */
  supportIsPhrase?: boolean;
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
    const message = pickMessage(`${which}Break`, snapshot.pending?.id ?? 0, t);
    return {
      label: t(`main.break.${which}.label`),
      time: formatDuration(snapshot.remainingMs),
      support: message.text,
      supportIsPhrase: message.phrase,
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
  /** True when `body` is a classical phrase (typeset in Naskh). */
  bodyIsPhrase: boolean;
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
export function describePopup(
  content: PopupContent,
  t: Translator = ENGLISH,
  /** The popup's seq; varies the message of popups that have no reminder id. */
  seed = 0,
): PopupCopy {
  switch (content.type) {
    case "reminder": {
      const message = pickMessage(
        content.reminder.kind === "movement" ? "movementReminder" : "eyesReminder",
        content.reminder.id,
        t,
      );
      return {
        title: reminderCopy(content.reminder.kind, t).title,
        body: message.text,
        bodyIsPhrase: message.phrase,
        kind: content.reminder.kind,
        actionable: true,
        eyebrow: t("popup.eyebrow"),
      };
    }
    case "test":
      return {
        ...reminderCopy(content.kind, t),
        bodyIsPhrase: false,
        kind: content.kind,
        actionable: false,
        eyebrow: t("popup.preview"),
      };
    case "breakOver": {
      const message = pickMessage("breakOver", seed, t);
      return {
        title: t("reminder.breakOver.title"),
        body: message.text,
        bodyIsPhrase: message.phrase,
        kind: content.kind,
        actionable: false,
        eyebrow: t("popup.eyebrow"),
      };
    }
  }
}

function reminderCopy(kind: ReminderKind, t: Translator): { title: string; body: string } {
  const which = kind === "movement" ? "movement" : "eyes";
  return {
    title: t(`reminder.${which}.title`),
    body: t(`reminder.${which}.body`),
  };
}

/**
 * Contextual messages. Each context mixes Breathe's own wording with the
 * classical phrases that suit it; the phrases are shown as ordinary messages,
 * never attributed (their sources are kept in content/phrases.sources.json,
 * which the app does not load).
 */
export const MESSAGE_POOLS = {
  eyesReminder: ["reminder.eyes.body", "phrase.eyesRight"],
  movementReminder: ["reminder.movement.body", "phrase.bodyRight"],
  eyesBreak: ["main.break.eyes.support", "phrase.timeAndTime", "phrase.selfRight"],
  movementBreak: ["main.break.movement.support", "phrase.timeAndTime", "phrase.selfRight"],
  breakOver: ["reminder.breakOver.body", "phrase.pastTime", "phrase.notAnHour"],
} as const satisfies Record<string, readonly MessageKey[]>;

export type MessageContext = keyof typeof MESSAGE_POOLS;

/**
 * Picks the message for `context`, keyed by a value that changes with every
 * event (a reminder id or a popup seq). Deterministic, so a re-render never
 * swaps the text, and consecutive events walk through the pool instead of
 * repeating the same line.
 */
export function contextualMessage(
  context: MessageContext,
  seed: number,
  t: Translator = ENGLISH,
): string {
  return pickMessage(context, seed, t).text;
}

export interface PickedMessage {
  text: string;
  /** True for a classical phrase, which is typeset in Naskh. */
  phrase: boolean;
}

export function pickMessage(
  context: MessageContext,
  seed: number,
  t: Translator = ENGLISH,
): PickedMessage {
  const pool: readonly MessageKey[] = MESSAGE_POOLS[context];
  const index = ((Math.trunc(seed) % pool.length) + pool.length) % pool.length;
  const key = pool[index] ?? pool[0]!;
  return { text: t(key), phrase: key.startsWith("phrase.") };
}
