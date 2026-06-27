# Analysis and Safety Rules

## Establish Data Health

Start with diagnostics. State degraded or unavailable window tracking, stale tracker samples, and disconnected browser reporting before presenting conclusions. Historical browser data may still exist while the bridge is currently disconnected.

## Time Semantics

- Timestamps and durations are milliseconds.
- Today uses the machine's local day boundary.
- Week uses local time and starts Monday.
- Today/week/range summaries include closed sessions and the active session, clipped to their range.
- Session queries return closed sessions only, filter on `start_time`, and do not clip rows to range boundaries.
- The active session has realtime duration sampled at `sampled_at_ms`.
- Do not add the active session to a summary that already includes it.

For exact custom totals, prefer `/api/v1/summary/range`. For forensic detail, use sessions and state their filtering semantics.

## Privacy

- Use aggregate app/category/domain totals before raw window titles or URLs.
- Treat window titles, paths, domains, and URLs as private local data.
- Respect `url: null`; never reconstruct a hidden URL from a title or domain.
- Do not repeat sensitive query strings, document names, or private titles unless directly required.
- Never reveal the API token.

## Interpretation

- Separate observations from hypotheses.
- Do not infer productivity, intent, or task completion from an app, domain, or title alone.
- Note missing coverage, AFK uncertainty, exclusions, and browser-bridge gaps.
- Avoid double counting overlapping desktop and browser records; browser activity enriches the foreground browser session rather than adding independent active time.
- Compare periods only when their boundaries and coverage are comparable.

## Writes

Classification, rename, exclusion, tracker settings, and future Tools actions change local state. Require explicit user intent before any write. Explain that exclusion changes future statistical interpretation, verify the exact executable name, perform the smallest requested mutation, and report the result.

## Response Shape

Present:

1. Requested time scope and data health.
2. High-signal findings with durations or percentages.
3. Relevant browser/domain or session detail only when needed.
4. Caveats and missing coverage.
5. Any executed write, clearly separated from analysis.
