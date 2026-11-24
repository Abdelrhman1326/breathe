import { useBreathe } from "@/hooks/useBreathe";
import { MainWindow } from "@/components/MainWindow";
import { SettingsWindow } from "@/components/settings/SettingsWindow";
import { PopupWindow } from "@/components/PopupWindow";
import { I18nProvider } from "@/i18n/I18nProvider";

/**
 * Both windows load the same bundle and route on the Tauri window label. The
 * settings window is created lazily by Rust the first time it is opened.
 */
export default function App() {
  const controller = useBreathe();
  return (
    <I18nProvider language={controller.settings?.language ?? "en"}>
      <WindowContent controller={controller} />
    </I18nProvider>
  );
}

function WindowContent({ controller }: { controller: ReturnType<typeof useBreathe> }) {
  if (!controller.ready || !controller.settings || !controller.snapshot) {
    // Keep the frame painted in the right colour while state loads, so the
    // window never flashes white.
    return <div className="h-full w-full bg-app" />;
  }

  if (controller.windowLabel === "popup") {
    return (
      <PopupWindow
        snoozeEnabled={controller.settings.snoozeEnabled}
        soundEnabled={controller.settings.soundEnabled}
        onTakeBreak={(reminderId) => {
          void controller.takeBreak(reminderId);
        }}
        onSnooze={(reminderId) => {
          void controller.snooze(reminderId);
        }}
      />
    );
  }

  if (controller.windowLabel === "settings") {
    return <SettingsWindow controller={controller} />;
  }

  return (
    <MainWindow
      settings={controller.settings}
      snapshot={controller.snapshot}
      actions={{
        onPause: () => {
          void controller.pause();
        },
        onResume: () => {
          void controller.resume();
        },
        onSetInterval: (minutes: number) => {
          void controller.update({ intervalMinutes: minutes });
        },
        onTakeBreak: (reminderId: number) => {
          void controller.takeBreak(reminderId);
        },
        onSnooze: (reminderId: number) => {
          void controller.snooze(reminderId);
        },
        onOpenSettings: () => {
          void controller.openSettings();
        },
        onHide: () => {
          void controller.hide();
        },
        onMinimize: () => {
          void controller.minimize();
        },
      }}
    />
  );
}
