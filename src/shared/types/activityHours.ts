import type { AppCategory } from "../classification/categoryTokens.ts";

/** Confirmed quantities; clients may format them but must not infer clock placement. */
export interface ActivityHour {
  hour: number;
  duration: number;
  categories: Array<{category: AppCategory; duration: number}>;
}
