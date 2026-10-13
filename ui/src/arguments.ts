// The command-argument rules (ADR-0045): the pure part, lifted out of `main.ts` so a test can reach
// them — `main.ts` queries the DOM at module load and cannot be imported at all.
//
// One rule decides what the chip shows, which option the picker marks, and what the command runs
// with: the remembered value when it still names an option, else the declared default, else the first
// option. It is one function so those three surfaces cannot disagree — and so a stale memory (a
// command whose options changed) degrades to the declaration instead of sending a value nothing
// accepts.

import type { ArgumentOption, CommandArgument } from "./types";

/**
 * The option an argument is on. `undefined` only for a declaration with no options — the panel shows
 * no chip then (a control with nothing to choose would be a control that lies).
 */
export function effectiveOption(
  argument: CommandArgument,
  remembered: string | undefined,
): ArgumentOption | undefined {
  const named = (id: string | undefined) =>
    id === undefined ? undefined : argument.options.find((option) => option.id === id);
  return named(remembered) ?? named(argument.default) ?? argument.options[0];
}

/** The commands that declare at least one usable argument — the ones that get a chip. */
export function declaredArguments(
  command: { arguments?: CommandArgument[] } | undefined,
): CommandArgument[] {
  return (command?.arguments ?? []).filter((argument) => argument.options.length > 0);
}

/**
 * The values one invocation carries: every declared argument's effective option, by argument id. The
 * panel sends this on invoke (and on a re-run's item action), so the Extension never has to know
 * which surface ran it or whether the user touched the chip at all.
 */
export function argumentValues(
  command: { id: string; arguments?: CommandArgument[] } | undefined,
  remembered: (commandId: string, argumentId: string) => string | undefined,
): Record<string, string> {
  const values: Record<string, string> = {};
  if (!command) return values;
  for (const argument of declaredArguments(command)) {
    const option = effectiveOption(argument, remembered(command.id, argument.id));
    if (option) values[argument.id] = option.id;
  }
  return values;
}
