/**
 * Localization.
 *
 * One flat JSON catalog per language in `src/locales/`, shared with Rust
 * (`src-tauri/src/i18n.rs` embeds the same files), so the tray, tooltips and
 * fallback notifications always agree with the interface.
 *
 * - Keys are typed from `en.json`, so a typo is a compile error.
 * - `{name}` placeholders are filled from `vars`; numbers are formatted for
 *   the language.
 * - Plurals use `Intl.PluralRules`: a plural message is stored as
 *   `base.one`, `base.other`, … (Arabic also uses `zero`, `two`, `few`,
 *   `many`) and requested with `plural("base", count)`.
 * - A key missing from a catalog falls back to English, so an incomplete
 *   translation degrades instead of showing a raw key.
 */
import ar from "@/locales/ar.json";
import en from "@/locales/en.json";

export type Language = "en" | "ar";
export type Direction = "ltr" | "rtl";
export type MessageKey = keyof typeof en;

/** Plural message families, derived from the keys that end in `.other`. */
export type PluralBase = MessageKey extends infer K
  ? K extends `${infer Base}.other`
    ? Base
    : never
  : never;

export type Vars = Record<string, string | number>;

type Catalog = Partial<Record<MessageKey, string>> & Record<string, string | undefined>;

const catalogs: Record<Language, Catalog> = { en, ar };

/** Languages offered in Settings, each named in its own language. */
export const LANGUAGES: readonly Language[] = ["en", "ar"];

export interface Translator {
  (key: MessageKey, vars?: Vars): string;
  plural: (base: PluralBase, count: number, vars?: Vars) => string;
  number: (value: number) => string;
  language: Language;
  /** BCP 47 tag for Intl formatting, with the numbering system pinned. */
  locale: string;
  dir: Direction;
}

/**
 * Arabic prose uses Arabic-Indic digits (٤٥). The numbering system is pinned
 * because engines disagree on the default for plain `ar` (Node/ICU picks
 * Latin digits, some webviews Arabic-Indic), and the countdown itself stays
 * in Western digits for legibility.
 */
const FORMAT_LOCALE: Record<Language, string> = {
  en: "en",
  ar: "ar-u-nu-arab",
};

export function direction(language: Language): Direction {
  return language === "ar" ? "rtl" : "ltr";
}

export function createTranslator(language: Language): Translator {
  const catalog = catalogs[language];
  const numbers = new Intl.NumberFormat(FORMAT_LOCALE[language]);
  const rules = new Intl.PluralRules(language);

  const format = (template: string, vars?: Vars): string =>
    vars
      ? template.replace(/\{(\w+)\}/g, (match, name: string) => {
          const value = vars[name];
          if (value === undefined) return match;
          return typeof value === "number" ? numbers.format(value) : value;
        })
      : template;

  const lookup = (key: string): string => catalog[key] ?? (en as Catalog)[key] ?? key;

  const t = ((key: MessageKey, vars?: Vars) => format(lookup(key), vars)) as Translator;

  t.plural = (base, count, vars) => {
    const category = rules.select(count);
    const specific = `${base}.${category}`;
    const key = catalog[specific] !== undefined || (en as Catalog)[specific] !== undefined
      ? specific
      : `${base}.other`;
    return format(lookup(key), { count, ...vars });
  };
  t.number = (value) => numbers.format(value);
  t.language = language;
  t.locale = FORMAT_LOCALE[language];
  t.dir = direction(language);
  return t;
}

/** Every catalog, for parity tests. */
export const CATALOGS: Readonly<Record<Language, Catalog>> = catalogs;
