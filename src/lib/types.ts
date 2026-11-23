/**
 * Wire types shared with the Rust backend.
 *
 * Every Rust struct in `src-tauri/src/model.rs` that crosses the IPC boundary is
 * serialised as camelCase, so these names must stay in sync with it.
 */

export type ReminderKind = "eyes" | "movement";

export type ThemePreference = "system" | "light" | "dark";

/**
 * Mirrors `ScheduleState` in Rust. `quit` is only observable while shutting down.
 * `awaiting`: a reminder is due and waits for the user's choice (no clock runs).
 * `break`: the user is taking the break they chose.
 */
export type ScheduleState =
  | "running"
  | "paused"
  | "snoozed"
  | "awaiting"
  | "break"
  | "quit";

/** Mirrors `BREAK_MINUTES` / `DEFAULT_SNOOZE_MINUTES` in Rust. */
export const BREAK_MINUTES = 3;
export const SNOOZE_MINUTES = 10;

/** A due reminder. Choices name its id so a repeated click is ignored. */
export interface PendingReminder {
  id: number;
  kind: ReminderKind;
}

export const INTERVAL_OPTIONS = [20, 30, 45, 60] as const;
export type IntervalMinutes = (typeof INTERVAL_OPTIONS)[number];

export const DEFAULT_INTERVAL_MINUTES: IntervalMinutes = 45;

export interface Settings {
  intervalMinutes: number;
  eyesEnabled: boolean;
  movementEnabled: boolean;
  snoozeEnabled: boolean;
  theme: ThemePreference;
  startWithSession: boolean;
  /** Whether scheduling should be active after the next launch. */
  startRunning: boolean;
}

export type SettingsPatch = Partial<Settings>;

export interface Snapshot {
  state: ScheduleState;
  /** Orthogonal to `state`: a running timer can be hidden and still fire. */
  windowHidden: boolean;
  intervalMinutes: number;
  remainingMs: number;
  /** 0..1, fraction of the interval still remaining. */
  remainingFraction: number;
  nextKind: ReminderKind | null;
  /** Unix epoch milliseconds of the next reminder, if one is scheduled. */
  nextAtMs: number | null;
  snoozeRemainingMs: number | null;
  /** The reminder waiting for a choice (`awaiting`) or being acted on (`break`). */
  pending: PendingReminder | null;
}

export interface NotificationStatus {
  available: boolean;
  service: string | null;
  vendor: string | null;
  version: string | null;
  specVersion: string | null;
  error: string | null;
}

export interface AutostartStatus {
  enabled: boolean;
  /** Platform mechanism, for example "XDG autostart" or "Windows Run key". */
  mechanism: string;
  entryPath: string | null;
  /** False on Linux sessions that never read XDG autostart entries. */
  sessionReadsStartupEntry: boolean;
  notes: string[];
}

export interface AppInfo {
  name: string;
  version: string;
  description: string;
  tauriVersion: string;
  platform: string;
}

export interface TestNotificationResult {
  delivered: boolean;
  kind: ReminderKind;
  error: string | null;
}
