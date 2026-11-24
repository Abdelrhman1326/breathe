import { describe, expect, it } from "vitest";
import ar from "./ar.json";
import en from "./en.json";
import { createTranslator } from "@/lib/i18n";

const placeholders = (text: string) => new Set(text.match(/\{\w+\}/g) ?? []);
const PLURAL_SUFFIX = /\.(zero|one|two|few|many|other)$/;

describe("translation catalogs", () => {
  it("Arabic translates every English key", () => {
    const missing = Object.keys(en).filter((key) => !(key in ar));
    expect(missing).toEqual([]);
  });

  it("Arabic has no keys English lacks, except extra plural forms", () => {
    const extra = Object.keys(ar).filter(
      (key) => !(key in en) && !(PLURAL_SUFFIX.test(key) && `${key.replace(PLURAL_SUFFIX, "")}.other` in en),
    );
    expect(extra).toEqual([]);
  });

  it("never introduces a placeholder the English message does not provide", () => {
    for (const [key, value] of Object.entries(ar)) {
      const base = key.replace(PLURAL_SUFFIX, ".other");
      const english = (en as Record<string, string>)[key] ?? (en as Record<string, string>)[base];
      if (english === undefined) continue;
      const allowed = placeholders(english);
      for (const name of placeholders(value)) {
        expect(allowed.has(name), `${key} uses ${name}`).toBe(true);
      }
    }
  });

  it("has no empty messages", () => {
    for (const catalog of [en, ar]) {
      for (const [key, value] of Object.entries(catalog)) {
        expect(value.trim(), key).not.toBe("");
      }
    }
  });
});

describe("Arabic", () => {
  const t = createTranslator("ar");

  it("uses the right plural form for each interval", () => {
    expect(t.plural("unit.minutes", 1)).toBe("دقيقة واحدة");
    expect(t.plural("unit.minutes", 2)).toBe("دقيقتان");
    expect(t.plural("unit.minutes", 3)).toBe("٣ دقائق");
    expect(t.plural("unit.minutes", 45)).toBe("٤٥ دقيقة");
  });

  it("reads right to left", () => {
    expect(t.dir).toBe("rtl");
  });
});

describe("contextual phrases", () => {
  // Names of narrators, speakers and books. The interface shows phrases only;
  // attribution lives in content/phrases.sources.json, never in a catalog.
  const ATTRIBUTION = [
    "البخاري", "مسلم", "رواه", "حديث", "القشيري", "ابن عقيل", "ابن الجوزي", "ابن رجب",
    "سلمان", "حنظلة", "Bukhari", "Muslim", "Qushayri", "Ibn ", "hadith", "narrated",
  ];

  it("never carries attribution in any catalog", () => {
    for (const catalog of [en, ar]) {
      for (const [key, value] of Object.entries(catalog)) {
        for (const name of ATTRIBUTION) {
          expect(value.includes(name), `${key} mentions ${name}`).toBe(false);
        }
      }
    }
  });
});
