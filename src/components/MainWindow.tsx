import { useMemo } from "react";
import { intervalLabel } from "@/lib/format";
import {
  breakActionLabel,
  describeMainWindow,
  primaryActionAriaLabel,
  primaryActionLabel,
  snoozeActionLabel,
} from "@/lib/presentation";
import { useT } from "@/i18n/I18nProvider";
import { INTERVAL_OPTIONS, type Settings, type Snapshot } from "@/lib/types";
import { CircularTimer } from "./CircularTimer";
import { TitleBar } from "./TitleBar";
import { ClockIcon, EyeIcon, MovementIcon, PauseIcon, PlayIcon, SettingsIcon } from "./icons";
import { IconButton, PrimaryButton, SecondaryButton, SelectField } from "./ui";

export interface MainWindowActions {
  onPause: () => void;
  onResume: () => void;
  onSetInterval: (minutes: number) => void;
  /** Takes the break for the waiting reminder, identified by id. */
  onTakeBreak: (reminderId: number) => void;
  /** Snoozes the waiting reminder, identified by id. */
  onSnooze: (reminderId: number) => void;
  onOpenSettings: () => void;
  onHide: () => void;
  onMinimize: () => void;
}

interface MainWindowProps {
  settings: Settings;
  snapshot: Snapshot;
  actions: MainWindowActions;
}

export function MainWindow({ settings, snapshot, actions }: MainWindowProps) {
  const t = useT();
  const copy = useMemo(
    () => describeMainWindow(settings, snapshot, t),
    [settings, snapshot, t],
  );

  const intervalOptions = useMemo(
    () =>
      INTERVAL_OPTIONS.map((minutes) => ({
        value: String(minutes),
        label: intervalLabel(minutes, t),
      })),
    [t],
  );

  const resuming = copy.phase === "paused" || copy.phase === "snoozed";
  const primaryLabel = primaryActionLabel(copy.phase, t);
  const primaryAria = primaryActionAriaLabel(copy.phase, t);
  const waiting = copy.phase === "awaiting" ? snapshot.pending : null;
  const offerSnooze = waiting !== null && settings.snoozeEnabled;

  return (
    <div className="flex h-full flex-col bg-app">
      <TitleBar onMinimize={actions.onMinimize} onHide={actions.onHide} />

      <main className="flex min-h-0 flex-1 flex-col px-6 pb-5">
        <div className="flex min-h-0 flex-1 items-center justify-center">
          <CircularTimer
            className="w-[234px] max-w-full"
            timeText={copy.time}
            label={copy.label}
            remainingFraction={snapshot.remainingFraction}
            muted={copy.muted}
          />
        </div>

        <p
          className="mx-auto mt-4 min-h-[40px] max-w-[292px] text-center text-[13px] leading-[1.55] text-muted"
          aria-live="polite"
        >
          {copy.support}
        </p>

        {waiting ? (
          <PrimaryButton
            className="mt-4"
            onClick={() => actions.onTakeBreak(waiting.id)}
            title={breakActionLabel(t)}
          >
            {waiting.kind === "movement" ? (
              <MovementIcon className="h-[16px] w-[16px]" />
            ) : (
              <EyeIcon className="h-[16px] w-[16px]" />
            )}
            {breakActionLabel(t)}
          </PrimaryButton>
        ) : (
          <PrimaryButton
            className="mt-4"
            onClick={resuming ? actions.onResume : actions.onPause}
            title={primaryAria}
            ariaLabel={primaryAria}
            disabled={copy.phase === "disabled"}
          >
            {resuming ? (
              <PlayIcon className="h-[15px] w-[15px]" />
            ) : (
              <PauseIcon className="h-[15px] w-[15px]" />
            )}
            {primaryLabel}
          </PrimaryButton>
        )}

        <div className="mt-3 flex items-stretch gap-2.5">
          {offerSnooze ? (
            <SecondaryButton
              className="flex-1"
              size="lg"
              onClick={() => actions.onSnooze(waiting.id)}
              title={snoozeActionLabel(t)}
            >
              <ClockIcon className="h-[16px] w-[16px]" />
              {snoozeActionLabel(t)}
            </SecondaryButton>
          ) : (
            <SelectField
              className="flex-1"
              label={t("interval.label")}
              value={String(snapshot.intervalMinutes)}
              options={intervalOptions}
              onChange={(value) => actions.onSetInterval(Number(value))}
            />
          )}
          <IconButton label={t("action.settings")} onClick={actions.onOpenSettings}>
            <SettingsIcon className="h-[18px] w-[18px]" />
          </IconButton>
        </div>
      </main>
    </div>
  );
}
