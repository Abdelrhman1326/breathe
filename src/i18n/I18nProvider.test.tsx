import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { I18nProvider, Rich, useT } from "./I18nProvider";

function Probe() {
  const t = useT();
  return <span>{t("action.settings")}</span>;
}

describe("<I18nProvider />", () => {
  it("sets the document language and direction", () => {
    const { rerender } = render(
      <I18nProvider language="en">
        <Probe />
      </I18nProvider>,
    );
    expect(document.documentElement.lang).toBe("en");
    expect(document.documentElement.dir).toBe("ltr");

    rerender(
      <I18nProvider language="ar">
        <Probe />
      </I18nProvider>,
    );
    expect(document.documentElement.lang).toBe("ar");
    expect(document.documentElement.dir).toBe("rtl");
  });

  it("defaults to English outside a provider", () => {
    render(<Probe />);
    expect(screen.getByText("Settings")).toBeTruthy();
  });
});

describe("<Rich />", () => {
  it("places elements where the translation puts their slots", () => {
    render(
      <p data-testid="rich">
        <Rich text="Remove {line} from {file}." parts={{ line: <code>x</code>, file: <code>y</code> }} />
      </p>,
    );
    const p = screen.getByTestId("rich");
    expect(p.textContent).toBe("Remove x from y.");
    expect(p.querySelectorAll("code")).toHaveLength(2);
  });
});
