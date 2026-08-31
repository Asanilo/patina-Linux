use std::collections::{BTreeMap, BTreeSet};

pub const HOUR_MS: i64 = 60 * 60 * 1000;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ActivityOrigin {
    Native,
    ImportExact,
    ImportBucket,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnedActivityRange<T> {
    pub origin: ActivityOrigin,
    pub start_ms: i64,
    pub end_ms: i64,
    pub capacity_end_ms: Option<i64>,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivityContribution<T> {
    pub origin: ActivityOrigin,
    pub duration_ms: i64,
    pub value: T,
}

#[derive(Clone)]
struct IndexedRange<T> {
    index: usize,
    range: OwnedActivityRange<T>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Interval {
    start_ms: i64,
    end_ms: i64,
}

/// Resolves exact precedence and returns duration-only contributions for one query range.
pub fn summarize_activity_range<T: Clone>(
    records: &[OwnedActivityRange<T>],
    from_ms: i64,
    to_ms: i64,
) -> Vec<ActivityContribution<T>> {
    if to_ms <= from_ms {
        return Vec::new();
    }

    let indexed = records
        .iter()
        .cloned()
        .enumerate()
        .filter(|(_, range)| range.end_ms > range.start_ms)
        .map(|(index, range)| IndexedRange { index, range })
        .collect::<Vec<_>>();
    let native = indexed
        .iter()
        .filter(|candidate| candidate.range.origin == ActivityOrigin::Native)
        .cloned()
        .collect::<Vec<_>>();
    let mut exact = indexed
        .iter()
        .filter(|candidate| candidate.range.origin == ActivityOrigin::ImportExact)
        .cloned()
        .collect::<Vec<_>>();
    exact.sort_by_key(sort_key);
    let resolved_exact = resolve_exact_ranges(&native, &exact);
    let exact_ranges = native
        .iter()
        .chain(resolved_exact.iter())
        .cloned()
        .collect::<Vec<_>>();
    let occupied = merge_intervals(
        exact_ranges
            .iter()
            .map(|candidate| Interval {
                start_ms: candidate.range.start_ms,
                end_ms: candidate.range.end_ms,
            })
            .collect(),
    );
    let mut contributions = exact_ranges
        .into_iter()
        .filter_map(|candidate| {
            let duration_ms = overlap_duration(
                candidate.range.start_ms,
                candidate.range.end_ms,
                from_ms,
                to_ms,
            );
            (duration_ms > 0).then_some(ActivityContribution {
                origin: candidate.range.origin,
                duration_ms,
                value: candidate.range.value,
            })
        })
        .collect::<Vec<_>>();

    let mut buckets_by_window: BTreeMap<(i64, i64), Vec<IndexedRange<T>>> = BTreeMap::new();
    for candidate in indexed
        .into_iter()
        .filter(|candidate| candidate.range.origin == ActivityOrigin::ImportBucket)
    {
        let capacity_end_ms = candidate
            .range
            .capacity_end_ms
            .unwrap_or(candidate.range.end_ms);
        if capacity_end_ms > candidate.range.start_ms {
            buckets_by_window
                .entry((candidate.range.start_ms, capacity_end_ms))
                .or_default()
                .push(candidate);
        }
    }

    // Bucket facts have duration but no intra-window position, so scale them to the
    // visible window before sharing capacity left by exact facts.
    for ((window_start_ms, window_end_ms), mut group) in buckets_by_window {
        let scoped_start_ms = window_start_ms.max(from_ms);
        let scoped_end_ms = window_end_ms.min(to_ms);
        if scoped_end_ms <= scoped_start_ms {
            continue;
        }
        group.sort_by_key(|candidate| candidate.index);
        let window_duration = window_end_ms - window_start_ms;
        let scoped_duration = scoped_end_ms - scoped_start_ms;
        let occupied_duration = intersected_duration(&occupied, scoped_start_ms, scoped_end_ms);
        let mut available_duration = (scoped_duration - occupied_duration).max(0);
        let requested = group
            .iter()
            .map(|candidate| {
                let full_request = candidate.range.end_ms - candidate.range.start_ms;
                full_request.saturating_mul(scoped_duration) / window_duration
            })
            .collect::<Vec<_>>();
        let mut remaining_requested = requested.iter().sum::<i64>();

        for (candidate, requested_duration) in group.into_iter().zip(requested) {
            let allocated = if remaining_requested <= available_duration {
                requested_duration
            } else if remaining_requested > 0 {
                requested_duration.saturating_mul(available_duration) / remaining_requested
            } else {
                0
            };
            if allocated > 0 {
                contributions.push(ActivityContribution {
                    origin: ActivityOrigin::ImportBucket,
                    duration_ms: allocated,
                    value: candidate.range.value,
                });
            }
            remaining_requested -= requested_duration;
            available_duration -= allocated;
        }
    }

    contributions
}

fn sort_key<T>(candidate: &IndexedRange<T>) -> (i64, ActivityOrigin, usize, i64) {
    (
        candidate.range.start_ms,
        candidate.range.origin,
        candidate.index,
        candidate.range.end_ms,
    )
}

fn resolve_exact_ranges<T: Clone>(
    native: &[IndexedRange<T>],
    exact: &[IndexedRange<T>],
) -> Vec<IndexedRange<T>> {
    #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
    enum Boundary {
        End,
        Start,
    }
    #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
    struct Event {
        time_ms: i64,
        boundary: Boundary,
        origin: ActivityOrigin,
        candidate_index: usize,
    }

    let mut events = Vec::with_capacity((native.len() + exact.len()) * 2);
    for candidate in native.iter().chain(exact.iter()) {
        events.push(Event {
            time_ms: candidate.range.start_ms,
            boundary: Boundary::Start,
            origin: candidate.range.origin,
            candidate_index: candidate.index,
        });
        events.push(Event {
            time_ms: candidate.range.end_ms,
            boundary: Boundary::End,
            origin: candidate.range.origin,
            candidate_index: candidate.index,
        });
    }
    events.sort_by_key(|event| event.time_ms);

    let exact_by_index = exact
        .iter()
        .map(|candidate| (candidate.index, candidate))
        .collect::<BTreeMap<_, _>>();
    let mut active_exact = BTreeSet::new();
    let mut active_native_count = 0_i64;
    let mut resolved: Vec<IndexedRange<T>> = Vec::new();
    let mut cursor = 0;
    while cursor < events.len() {
        let time_ms = events[cursor].time_ms;
        while cursor < events.len() && events[cursor].time_ms == time_ms {
            let event = events[cursor];
            if event.origin == ActivityOrigin::Native {
                active_native_count += if event.boundary == Boundary::Start {
                    1
                } else {
                    -1
                };
            } else if event.boundary == Boundary::Start {
                if let Some(candidate) = exact_by_index.get(&event.candidate_index) {
                    active_exact.insert(sort_key(candidate));
                }
            } else if let Some(candidate) = exact_by_index.get(&event.candidate_index) {
                active_exact.remove(&sort_key(candidate));
            }
            cursor += 1;
        }

        let Some(next_time_ms) = events.get(cursor).map(|event| event.time_ms) else {
            break;
        };
        if next_time_ms <= time_ms || active_native_count > 0 {
            continue;
        }
        let Some((_, _, winner_index, _)) = active_exact.iter().next().copied() else {
            continue;
        };
        let Some(winner) = exact_by_index.get(&winner_index) else {
            continue;
        };
        if let Some(previous) = resolved.last_mut() {
            if previous.index == winner_index && previous.range.end_ms == time_ms {
                previous.range.end_ms = next_time_ms;
                continue;
            }
        }
        let mut segment = (*winner).clone();
        segment.range.start_ms = time_ms;
        segment.range.end_ms = next_time_ms;
        resolved.push(segment);
    }
    resolved
}

fn merge_intervals(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.retain(|interval| interval.end_ms > interval.start_ms);
    intervals.sort_by_key(|interval| (interval.start_ms, interval.end_ms));
    let mut merged: Vec<Interval> = Vec::new();
    for interval in intervals {
        if let Some(previous) = merged.last_mut() {
            if interval.start_ms <= previous.end_ms {
                previous.end_ms = previous.end_ms.max(interval.end_ms);
                continue;
            }
        }
        merged.push(interval);
    }
    merged
}

fn intersected_duration(intervals: &[Interval], start_ms: i64, end_ms: i64) -> i64 {
    intervals
        .iter()
        .map(|interval| overlap_duration(interval.start_ms, interval.end_ms, start_ms, end_ms))
        .sum()
}

fn overlap_duration(start_ms: i64, end_ms: i64, from_ms: i64, to_ms: i64) -> i64 {
    (end_ms.min(to_ms) - start_ms.max(from_ms)).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(
        origin: ActivityOrigin,
        start_ms: i64,
        end_ms: i64,
        capacity_end_ms: Option<i64>,
        value: &'static str,
    ) -> OwnedActivityRange<&'static str> {
        OwnedActivityRange {
            origin,
            start_ms,
            end_ms,
            capacity_end_ms,
            value,
        }
    }

    #[test]
    fn native_masks_exact_and_reduces_bucket_capacity() {
        let result = summarize_activity_range(
            &[
                range(ActivityOrigin::ImportExact, 0, 60, None, "exact"),
                range(ActivityOrigin::Native, 10, 30, None, "native"),
                range(ActivityOrigin::ImportBucket, 0, 40, Some(100), "bucket-a"),
                range(ActivityOrigin::ImportBucket, 0, 40, Some(100), "bucket-b"),
            ],
            0,
            100,
        );

        let total = result.iter().map(|item| item.duration_ms).sum::<i64>();
        assert_eq!(total, 100);
        assert_eq!(
            result
                .iter()
                .filter(|item| item.origin == ActivityOrigin::Native)
                .map(|item| item.duration_ms)
                .sum::<i64>(),
            20
        );
        assert_eq!(
            result
                .iter()
                .filter(|item| item.origin == ActivityOrigin::ImportExact)
                .map(|item| item.duration_ms)
                .sum::<i64>(),
            40
        );
    }

    #[test]
    fn partial_bucket_range_is_prorated_before_capacity_is_applied() {
        let result = summarize_activity_range(
            &[
                range(ActivityOrigin::Native, 0, 20, None, "native"),
                range(ActivityOrigin::ImportBucket, 0, 60, Some(100), "bucket"),
            ],
            0,
            20,
        );

        assert_eq!(result.iter().map(|item| item.duration_ms).sum::<i64>(), 20);
        assert_eq!(
            result
                .iter()
                .filter(|item| item.origin == ActivityOrigin::ImportBucket)
                .map(|item| item.duration_ms)
                .sum::<i64>(),
            0
        );
    }

    #[test]
    fn overlapping_imported_exact_records_have_deterministic_winner() {
        let result = summarize_activity_range(
            &[
                range(ActivityOrigin::ImportExact, 0, 100, None, "first"),
                range(ActivityOrigin::ImportExact, 20, 80, None, "second"),
            ],
            0,
            100,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "first");
        assert_eq!(result[0].duration_ms, 100);
    }
}
