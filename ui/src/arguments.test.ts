// The command-argument rules (ADR-0045). Each test is a way the chip could lie: showing a value the
// command will not run with, forgetting what the user chose, or sending a remembered id that the
// declaration no longer offers.

import { describe, expect, it } from "vitest";
import { argumentValues, declaredArguments, effectiveOption } from "./arguments";
import type { CommandArgument, CommandMeta } from "./types";

/** Translate's own declaration: the auto direction by default, plus the two explicit ones. */
const language: CommandArgument = {
  id: "language",
  title: "Language",
  default: "auto",
  options: [
    { id: "auto", title: "Zh ↔ En" },
    { id: "en", title: "English" },
    { id: "zh", title: "Chinese" },
  ],
};

function command(overrides: Partial<CommandMeta> = {}): CommandMeta {
  return {
    id: "aicmd.translate",
    extensionId: "ai-commands",
    title: "Translate",
    input: "selection",
    live: false,
    ...overrides,
  };
}

describe("effectiveOption", () => {
  it("shows the declared default when nothing was chosen", () => {
    expect(effectiveOption(language, undefined)?.id).toBe("auto");
    expect(effectiveOption(language, undefined)?.title).toBe("Zh ↔ En");
  });

  it("keeps what the user chose", () => {
    expect(effectiveOption(language, "zh")?.title).toBe("Chinese");
  });

  it("falls back to the declaration when the remembered id is gone", () => {
    // A command whose options changed must not send a value nothing accepts.
    expect(effectiveOption(language, "klingon")?.id).toBe("auto");
  });

  it("uses the first option when the declaration names no default", () => {
    const noDefault: CommandArgument = { ...language, default: undefined };
    expect(effectiveOption(noDefault, undefined)?.id).toBe("auto");
  });

  it("uses the first option when the default names an option that is gone", () => {
    const stale: CommandArgument = { ...language, default: "klingon" };
    expect(effectiveOption(stale, undefined)?.id).toBe("auto");
  });

  it("has no option at all for a declaration with an empty list", () => {
    expect(effectiveOption({ ...language, options: [] }, "auto")).toBeUndefined();
  });
});

describe("declaredArguments", () => {
  it("is empty for a command that declares none", () => {
    expect(declaredArguments(command())).toEqual([]);
    expect(declaredArguments(undefined)).toEqual([]);
  });

  it("drops a declaration with nothing to choose", () => {
    // The chip is the promise of a choice; an empty option list cannot keep it.
    expect(declaredArguments(command({ arguments: [{ ...language, options: [] }] }))).toEqual([]);
    expect(declaredArguments(command({ arguments: [language] }))).toEqual([language]);
  });
});

describe("argumentValues", () => {
  it("sends every declared argument's effective value", () => {
    expect(argumentValues(command({ arguments: [language] }), () => undefined)).toEqual({
      language: "auto",
    });
    expect(argumentValues(command({ arguments: [language] }), () => "zh")).toEqual({
      language: "zh",
    });
  });

  it("sends nothing for a command that declares none", () => {
    expect(argumentValues(command(), () => "zh")).toEqual({});
    expect(argumentValues(undefined, () => "zh")).toEqual({});
  });

  it("asks the memory per command, so two commands cannot share a value", () => {
    const asked: string[] = [];
    argumentValues(command({ arguments: [language] }), (commandId, argumentId) => {
      asked.push(`${commandId}:${argumentId}`);
      return undefined;
    });
    expect(asked).toEqual(["aicmd.translate:language"]);
  });
});
