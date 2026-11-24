import { act, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PopupView } from "@/lib/types";

const playChime = vi.fn(() => Promise.resolve());
let emit: ((view: PopupView | null) => void) | null = null;

vi.mock("@/lib/chime", () => ({ playChime }));
vi.mock("@/lib/ipc", () => ({
  getPopup: () => Promise.resolve(null),
  dismissPopup: () => Promise.resolve(),
  onPopup: (handler: (view: PopupView | null) => void) => {
    emit = handler;
    return () => undefined;
  },
}));

const { PopupWindow } = await import("./PopupWindow");

function view(seq: number): PopupView {
  return { seq, content: { type: "reminder", reminder: { id: seq, kind: "eyes" } } };
}

describe("popup chime", () => {
  beforeEach(() => {
    playChime.mockClear();
    emit = null;
  });

  function mount(soundEnabled: boolean) {
    return render(
      <PopupWindow
        snoozeEnabled
        soundEnabled={soundEnabled}
        onTakeBreak={vi.fn()}
        onSnooze={vi.fn()}
      />,
    );
  }

  it("chimes once per new popup, not on repeats", async () => {
    mount(true);
    await act(async () => emit?.(view(1)));
    await act(async () => emit?.(view(1)));
    expect(playChime).toHaveBeenCalledTimes(1);

    await act(async () => emit?.(view(2)));
    expect(playChime).toHaveBeenCalledTimes(2);
  });

  it("stays silent when the sound is turned off", async () => {
    mount(false);
    await act(async () => emit?.(view(1)));
    expect(playChime).not.toHaveBeenCalled();
  });

  it("does not chime when the popup hides", async () => {
    mount(true);
    await act(async () => emit?.(view(1)));
    await act(async () => emit?.(null));
    expect(playChime).toHaveBeenCalledTimes(1);
  });
});
