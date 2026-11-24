import { describe, expect, it } from "vitest";
import { createTranslator, direction } from "./i18n";

describe("createTranslator", () => {
  const t = createTranslator("en");

  it("looks up messages and fills placeholders", () => {
    expect(t("app.name")).toBe("Breathe");
    expect(t("interval.every", { duration: "20 minutes" })).toBe("Every 20 minutes");
  });

  it("formats numeric placeholders for the language", () => {
    expect(t("action.snooze", { count: 10 })).toBe("Snooze 10 minutes");
  });

  it("leaves an unknown placeholder visible rather than dropping it", () => {
    expect(t("interval.every")).toBe("Every {duration}");
  });

  it("selects plural forms with Intl.PluralRules", () => {
    expect(t.plural("unit.minutes", 1)).toBe("1 minute");
    expect(t.plural("unit.minutes", 45)).toBe("45 minutes");
  });

  it("falls back to English for a missing translation", () => {
    const arabic = createTranslator("ar");
    expect(arabic("app.name").length).toBeGreaterThan(0);
  });

  it("knows each language's direction", () => {
    expect(direction("en")).toBe("ltr");
    expect(direction("ar")).toBe("rtl");
    expect(createTranslator("ar").dir).toBe("rtl");
  });
});
