# macOS notes

Breathe builds for **macOS 11 (Big Sur) and later** as a universal app
(Apple silicon and Intel), packaged as `Breathe.app` inside a `.dmg`.

> **Status: preview.** The macOS code is built only on public Apple APIs and
> is type-checked and linted for `aarch64-apple-darwin`. It has **not been run
> on a Mac yet**: no Mac or macOS VM was available. Treat every behaviour below
> as implemented-but-unverified until the checklist at the end is done on real
> hardware.

## What is implemented

| Area | Implementation | Verified |
| --- | --- | --- |
| Build & package | `tauri.macos.conf.json`: `.app` + `.dmg`, minimum macOS 11, `icon.icns` | CI job builds a universal DMG and checks architectures, `LSMinimumSystemVersion` and the icon (runs on pull requests and `main`) |
| Lint & tests | Clippy `-D warnings`, Rust and frontend tests on `macos-14` | Clippy also passes locally for `aarch64-apple-darwin` (type-check only) |
| Quit | ⌘Q from the default app menu stops the scheduler cleanly (`RunEvent::Exit`) | Type-checked |
| Dock icon | Clicking it while the window is hidden brings the window back (`RunEvent::Reopen`) | Type-checked |
| Start at login | Per-user LaunchAgent `~/Library/LaunchAgents/Breathe.plist` via `tauri-plugin-autostart` | Type-checked |
| Menu bar | Monochrome template icon; **live countdown** (`MM:SS`) next to it, "Break" while a reminder waits, nothing when paused | `menu_bar_title` unit-tested on every platform; AppKit side type-checked |
| Reminder popup | Non-activating `orderFrontRegardless`, floating level, all Spaces incl. full screen, top-right of the work area, rounded window with system shadow | Type-checked |
| Sound | Web Audio chime in the popup (WKWebView autoplay is enabled by wry) | Not run on macOS |
| Arabic / RTL | Same webview UI as Linux/Windows; Arabic fonts bundled | Not run on macOS |
| Sleep/wake | `Instant` uses `CLOCK_UPTIME_RAW` on macOS, which stops during sleep, so the existing suspend detection applies | Reasoned from Rust's documented behaviour, not observed |

No private or undocumented API is used. In particular Tauri's
`macos-private-api` feature is **not** enabled: transparent webviews need it,
so the popup is rounded through its window layer instead.

## Installing an unsigned build

There is no Developer ID certificate yet, so the app is only ad-hoc signed and
Gatekeeper blocks the first launch. On macOS 15 and later the old Control-click
bypass is gone:

1. Open the DMG and drag Breathe to Applications.
2. Launch it once; macOS refuses.
3. Open **System Settings → Privacy & Security**, scroll to the Breathe message
   and choose **Open Anyway**, then confirm.

Signing with a Developer ID and notarizing the DMG (hardened runtime,
`notarytool`) will remove these steps; it needs an Apple Developer Program
membership and is not set up yet.

## Live Activities and the notch

* **ActivityKit on macOS** is listed by Apple as **macOS 27.3, beta**. Using it
  would need a separate WidgetKit extension target that Tauri's bundler cannot
  build, a team-signed build, and a minimum OS that almost nobody runs yet.
  Deferred until 27.3 is released and can be tried on hardware.
* **Menu bar countdown** (above) is the supported, always-visible timer on
  every Mac, with or without a notch.
* **Notch-adjacent pill** (a small non-activating panel beside the notch, found
  with `NSScreen.safeAreaInsets` / `auxiliaryTopLeftArea`) is possible with
  public APIs but needs a notched Mac to get right; it is a later step.

## Verification checklist (real Mac)

- [ ] DMG opens; app launches after "Open Anyway"; Dock icon and menu bar icon appear.
- [ ] Menu bar shows the countdown, updates every second, tints correctly in light and dark menu bars.
- [ ] Reminder popup appears top-right **without taking focus** from a text editor; buttons work (note whether the first click only activates).
- [ ] Popup corners are rounded with a shadow; it shows over a full-screen app and on every Space.
- [ ] Chime plays.
- [ ] Break / snooze / automatic restart after the break.
- [ ] ⌘Q quits; closing the window hides it; clicking the Dock icon restores it.
- [ ] Start at login creates and removes the LaunchAgent and works after a restart.
- [ ] Sleep across a deadline: no burst of reminders on wake.
- [ ] Arabic: RTL layout and phrases render correctly.
