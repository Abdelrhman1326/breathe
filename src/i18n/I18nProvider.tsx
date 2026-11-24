import {
  createContext,
  Fragment,
  useContext,
  useEffect,
  useMemo,
  type ReactNode,
} from "react";
import { createTranslator, type Language, type Translator } from "@/lib/i18n";

const english = createTranslator("en");

const I18nContext = createContext<Translator>(english);

/**
 * Provides the translator for the current language and keeps `<html lang dir>`
 * in sync, so text direction, hyphenation and screen-reader pronunciation follow
 * the language in every window.
 */
export function I18nProvider({
  language,
  children,
}: {
  language: Language;
  children: ReactNode;
}) {
  const t = useMemo(() => createTranslator(language), [language]);

  useEffect(() => {
    document.documentElement.lang = t.language;
    document.documentElement.dir = t.dir;
  }, [t]);

  return <I18nContext.Provider value={t}>{children}</I18nContext.Provider>;
}

/** The active translator. Outside a provider (tests) it is English. */
export function useT(): Translator {
  return useContext(I18nContext);
}

/**
 * Renders a translated template whose `{name}` slots hold elements, e.g. a
 * path in `<Mono>`. Word order stays in the translation, so Arabic can place
 * the element wherever its grammar needs it.
 */
export function Rich({
  text,
  parts,
}: {
  text: string;
  parts: Record<string, ReactNode>;
}) {
  const pieces = text.split(/\{(\w+)\}/);
  return (
    <>
      {pieces.map((piece, index) =>
        index % 2 === 1 ? (
          <Fragment key={index}>{parts[piece] ?? `{${piece}}`}</Fragment>
        ) : (
          piece
        ),
      )}
    </>
  );
}
