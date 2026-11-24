import { describe, expect, it } from "vitest";
import {
  describePopup,
  describeMainWindow,
  effectiveIntervalMinutes,
  primaryActionLabel,
  remindersDisabled,
} from "./presentation";
import type { Settings, Snapshot } from "./types";

const settings: Settings = {
  intervalMinutes: 45,
  eyesEnabled: true,
  movementEnabled: true,
  snoozeEnabled: true,
  soundEnabled: true,
  language: "en",
  theme: "system",
  startWithSession: false,
  startRunning: true,
};

function snapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    state: "running",
    windowHidden: false,
    intervalMinutes: 45,
    remainingMs: 32 * 60 * 1000 + 15 * 1000,
    remainingFraction: 32.25 / 45,
    nextKind: "eyes",
    nextAtMs: Date.UTC(2026, 3, 13, 10, 29),
    snoozeRemainingMs: null,
    pending: null,
    ...overrides,
  };
}

describe("describeMainWindow", () => {
  it("shows the real time remaining and names the upcoming category", () => {
    const copy = describeMainWindow(settings, snapshot());
    expect(copy.label).toBe("Next eye break in");
    expect(copy.time).toBe("32:15");
    expect(copy.phase).toBe("running");
    expect(copy.muted).toBe(false);
  });

  it("names movement as the next break while alternating", () => {
    const copy = describeMainWindow(settings, snapshot({ nextKind: "movement" }));
    expect(copy.label).toBe("Next movement break in");
  });

  it("falls back to a neutral caption when no category is available", () => {
    const copy = describeMainWindow(settings, snapshot({ nextKind: null }));
    expect(copy.label).toBe("Next break in");
  });

  it("renders the paused state without losing the countdown", () => {
    const copy = describeMainWindow(settings, snapshot({ state: "paused" }));
    expect(copy.phase).toBe("paused");
    expect(copy.label).toBe("Paused");
    expect(copy.time).toBe("32:15");
    expect(copy.muted).toBe(true);
  });

  it("counts down the snooze window and shows the resume time", () => {
    const copy = describeMainWindow(
      settings,
      snapshot({
        state: "snoozed",
        remainingMs: 5 * 60 * 1000,
        snoozeRemainingMs: 4 * 60 * 1000 + 30 * 1000,
      }),
    );
    expect(copy.phase).toBe("snoozed");
    expect(copy.label).toBe("Snoozed");
    expect(copy.time).toBe("04:30");
    expect(copy.support).toMatch(/^Next reminder at /);
  });

  it("asks for a choice when a reminder is due", () => {
    const copy = describeMainWindow(
      settings,
      snapshot({ state: "awaiting", remainingMs: 0, pending: { id: 1, kind: "eyes" } }),
    );
    expect(copy.phase).toBe("awaiting");
    expect(copy.label).toBe("Time to rest your eyes");
    expect(copy.time).toBe("00:00");
    expect(copy.muted).toBe(false);
  });

  it("names the movement reminder that is waiting", () => {
    const copy = describeMainWindow(
      settings,
      snapshot({ state: "awaiting", remainingMs: 0, pending: { id: 2, kind: "movement" } }),
    );
    expect(copy.label).toBe("Time to move a little");
  });

  it("counts the break down with its own caption", () => {
    const copy = describeMainWindow(
      settings,
      snapshot({
        state: "break",
        remainingMs: 2 * 60 * 1000,
        pending: { id: 2, kind: "movement" },
      }),
    );
    expect(copy.phase).toBe("break");
    expect(copy.label).toBe("Movement break");
    expect(copy.time).toBe("02:00");
  });

  it("hides the countdown when every reminder category is off", () => {
    const copy = describeMainWindow(
      { ...settings, eyesEnabled: false, movementEnabled: false },
      snapshot(),
    );
    expect(copy.phase).toBe("disabled");
    expect(copy.time).toBe("--:--");
    expect(copy.label).toBe("Reminders are off");
    expect(copy.muted).toBe(true);
  });
});

describe("primaryActionLabel", () => {
  it("offers the break while a reminder waits", () => {
    expect(primaryActionLabel("awaiting")).toBe("Take a 3-minute break");
    expect(primaryActionLabel("break")).toBe("Pause");
  });

  it("offers resume from paused and snoozed, otherwise pause", () => {
    expect(primaryActionLabel("running")).toBe("Pause");
    expect(primaryActionLabel("paused")).toBe("Resume");
    expect(primaryActionLabel("snoozed")).toBe("Resume now");
    expect(primaryActionLabel("disabled")).toBe("Pause");
  });
});

describe("remindersDisabled", () => {
  it("is only true when both categories are disabled", () => {
    expect(remindersDisabled(settings)).toBe(false);
    expect(remindersDisabled({ ...settings, eyesEnabled: false })).toBe(false);
    expect(
      remindersDisabled({ ...settings, movementEnabled: false, eyesEnabled: false }),
    ).toBe(true);
  });
});

describe("effectiveIntervalMinutes", () => {
  it("prefers the scheduler's interval over the stored setting", () => {
    expect(effectiveIntervalMinutes(settings, snapshot({ intervalMinutes: 60 }))).toBe(
      60,
    );
  });

  it("falls back to the stored setting when the snapshot has none", () => {
    expect(effectiveIntervalMinutes(settings, snapshot({ intervalMinutes: 0 }))).toBe(
      45,
    );
  });
});

describe("describePopup", () => {
  it("makes reminders actionable and names the category", () => {
    const copy = describePopup({
      type: "reminder",
      reminder: { id: 1, kind: "movement" },
    });
    expect(copy.title).toBe("Time to move a little");
    expect(copy.actionable).toBe(true);
    expect(copy.kind).toBe("movement");
  });

  it("uses the same reminder copy for the preview, without actions", () => {
    const preview = describePopup({ type: "test", kind: "eyes" });
    const real = describePopup({ type: "reminder", reminder: { id: 1, kind: "eyes" } });
    expect(preview.title).toBe(real.title);
    expect(preview.body).toBe(real.body);
    expect(preview.actionable).toBe(false);
    expect(preview.eyebrow).toBe("Preview");
  });

  it("welcomes the user back after a break", () => {
    const copy = describePopup({ type: "breakOver", kind: "eyes" });
    expect(copy.title).toBe("Break's over");
    expect(copy.actionable).toBe(false);
  });
});
