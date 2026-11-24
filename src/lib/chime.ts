/**
 * Breathe's notification chime, synthesised with the Web Audio API.
 *
 * No audio file and no native audio dependency: two soft sine partials (a
 * fifth apart) with a quick attack and a gentle exponential decay, kept quiet
 * so it reads as a nudge rather than an alarm.
 *
 * The webview allows playback without a user gesture (wry enables autoplay on
 * WebView2, WebKitGTK and WKWebView), so the hidden popup window can play it.
 */

/** Partials of the chime: frequency (Hz), start offset (s), peak gain. */
export const CHIME_PARTIALS: ReadonlyArray<readonly [number, number, number]> = [
  [659.25, 0, 0.12], // E5
  [987.77, 0.09, 0.08], // B5
];

/** How long each partial rings, in seconds. */
export const CHIME_DECAY = 1.4;

type AudioContextCtor = new () => AudioContext;

let context: AudioContext | null = null;

function audioContext(): AudioContext | null {
  if (context) return context;
  const Ctor: AudioContextCtor | undefined =
    typeof window === "undefined"
      ? undefined
      : (window.AudioContext ??
        (window as unknown as { webkitAudioContext?: AudioContextCtor })
          .webkitAudioContext);
  if (!Ctor) return null;
  context = new Ctor();
  return context;
}

/**
 * Plays the chime once. Resolves quietly if audio is unavailable: a missing
 * sound must never break the popup.
 */
export async function playChime(): Promise<void> {
  try {
    const ctx = audioContext();
    if (!ctx) return;
    if (ctx.state === "suspended") await ctx.resume();

    const start = ctx.currentTime + 0.02;
    for (const [frequency, offset, peak] of CHIME_PARTIALS) {
      const oscillator = ctx.createOscillator();
      const gain = ctx.createGain();
      const at = start + offset;

      oscillator.type = "sine";
      oscillator.frequency.setValueAtTime(frequency, at);
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(peak, at + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + CHIME_DECAY);

      oscillator.connect(gain).connect(ctx.destination);
      oscillator.start(at);
      oscillator.stop(at + CHIME_DECAY + 0.05);
    }
  } catch (error) {
    console.warn("[breathe] could not play the chime", error);
  }
}
