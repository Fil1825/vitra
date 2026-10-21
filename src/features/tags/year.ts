/** `''` → no year; otherwise a plausible year, or `undefined` if the input isn't one. */
export function parseYear(input: string): number | null | undefined {
  const trimmed = input.trim();
  if (trimmed === '') {
    return null;
  }
  const year = Number(trimmed);
  return Number.isInteger(year) && year > 0 && year < 10_000 ? year : undefined;
}
