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
    /// Clipped exact start, or scoped bucket start (not an observed position within a bucket).
    pub start_ms: i64,
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
    summarize_activity_partitions(records, &[from_ms, to_ms])
        .pop()
        .unwrap_or_default()
}

/// Resolves one scope once, then distributes duration quantities into consecutive
/// display partitions. Bucket contributions are never exact observed intervals.
/// Splitting the same scope conserves every record's allocated milliseconds.
pub fn summarize_activity_partitions<T: Clone>(
    records: &[OwnedActivityRange<T>],
    boundaries: &[i64],
) -> Vec<Vec<ActivityContribution<T>>> {
    if boundaries.len() < 2 || boundaries.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Vec::new();
    }
    let from_ms = boundaries[0];
    let to_ms = *boundaries.last().unwrap();

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
    let mut contributions = vec![Vec::new(); boundaries.len() - 1];
    for candidate in exact_ranges {
        for (index, partition) in boundaries.windows(2).enumerate() {
            let duration_ms = overlap_duration(
                candidate.range.start_ms,
                candidate.range.end_ms,
                partition[0],
                partition[1],
            );
            if duration_ms > 0 {
                contributions[index].push(ActivityContribution {
                    origin: candidate.range.origin,
                    start_ms: candidate.range.start_ms.max(partition[0]),
                    duration_ms,
                    value: candidate.range.value.clone(),
                });
            }
        }
    }

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
        let mut capacities: Vec<i64> = boundaries
            .windows(2)
            .map(|part| {
                let start = part[0].max(scoped_start_ms);
                let end = part[1].min(scoped_end_ms);
                if end <= start {
                    0
                } else {
                    (end - start - intersected_duration(&occupied, start, end)).max(0)
                }
            })
            .collect();
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
                let mut remainder = allocated;
                let mut remaining_capacity = available_duration;
                for (index, capacity) in capacities.iter_mut().enumerate() {
                    let original_capacity = *capacity;
                    let share = if remaining_capacity > 0 {
                        (i128::from(remainder) * i128::from(original_capacity)
                            / i128::from(remaining_capacity)) as i64
                    } else {
                        0
                    };
                    if share > 0 {
                        contributions[index].push(ActivityContribution {
                            origin: ActivityOrigin::ImportBucket,
                            start_ms: scoped_start_ms.max(boundaries[index]),
                            duration_ms: share,
                            value: candidate.range.value.clone(),
                        });
                    }
                    remainder -= share;
                    *capacity -= share;
                    remaining_capacity -= original_capacity;
                }
                debug_assert_eq!(remainder, 0);
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
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureCase {
        name: String,
        scope: FixtureScope,
        records: Vec<FixtureRecord>,
        expected_duration_by_key: HashMap<String, i64>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureScope {
        start_time: i64,
        end_time: i64,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureRecord {
        key: String,
        origin: String,
        start_time: i64,
        end_time: i64,
        capacity_end_time: Option<i64>,
    }

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
    fn partitions_preserve_small_bucket_quantities_instead_of_rounding_them_away() {
        let records = [range(ActivityOrigin::ImportBucket, 0, 1, Some(100), "one")];
        let partitions = summarize_activity_partitions(&records, &[0, 50, 100]);
        assert_eq!(
            partitions
                .iter()
                .flatten()
                .map(|value| value.duration_ms)
                .sum::<i64>(),
            1
        );
        assert_eq!(partitions[0].len(), 0);
        assert_eq!(partitions[1][0].duration_ms, 1);
        assert_eq!(partitions[1][0].origin, ActivityOrigin::ImportBucket);
        // Independent queries cannot retain the parent scope's integer remainder.
        assert!(summarize_activity_range(&records, 0, 50).is_empty());
        assert!(summarize_activity_range(&records, 50, 100).is_empty());
    }

    #[test]
    fn partitions_conserve_each_record_and_do_not_overfill_remaining_capacity() {
        for first in 0..=12 {
            for second in 0..=12 {
                for cut in 1..10 {
                    let records = [
                        range(ActivityOrigin::Native, 2, 4, None, "native"),
                        range(ActivityOrigin::ImportExact, 1, 5, None, "exact"),
                        range(ActivityOrigin::ImportBucket, 0, first, Some(10), "first"),
                        range(ActivityOrigin::ImportBucket, 0, second, Some(10), "second"),
                    ];
                    let sum = |values: Vec<ActivityContribution<&'static str>>| {
                        let mut totals = HashMap::new();
                        for value in values {
                            *totals.entry(value.value).or_insert(0) += value.duration_ms;
                        }
                        totals
                    };
                    let full = sum(summarize_activity_range(&records, 0, 10));
                    let partitions = summarize_activity_partitions(&records, &[0, cut, 10]);
                    for (part, capacity) in partitions.iter().zip([cut, 10 - cut]) {
                        assert!(
                            part.iter().map(|value| value.duration_ms).sum::<i64>() <= capacity
                        );
                    }
                    assert_eq!(sum(partitions.into_iter().flatten().collect()), full);
                }
            }
        }
        assert!(summarize_activity_partitions::<()>(&[], &[1, 1]).is_empty());
        assert!(summarize_activity_partitions::<()>(&[], &[1, 0]).is_empty());
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

    #[test]
    fn shared_fixture_matches_cross_runtime_activity_contract() {
        let fixtures: Vec<FixtureCase> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/activity-read-model-cases.json"
        ))
        .unwrap();

        for fixture in fixtures {
            let records = fixture
                .records
                .into_iter()
                .map(|record| OwnedActivityRange {
                    origin: match record.origin.as_str() {
                        "native" => ActivityOrigin::Native,
                        "import_exact" => ActivityOrigin::ImportExact,
                        "import_bucket" => ActivityOrigin::ImportBucket,
                        value => panic!("unsupported fixture origin: {value}"),
                    },
                    start_ms: record.start_time,
                    end_ms: record.end_time,
                    capacity_end_ms: record.capacity_end_time,
                    value: record.key,
                })
                .collect::<Vec<_>>();
            let contributions = summarize_activity_range(
                &records,
                fixture.scope.start_time,
                fixture.scope.end_time,
            );
            let mut actual = HashMap::<String, i64>::new();
            for contribution in contributions {
                *actual.entry(contribution.value).or_insert(0) += contribution.duration_ms;
            }
            assert_eq!(actual, fixture.expected_duration_by_key, "{}", fixture.name);
        }
    }

    #[test]
    fn shared_daily_fixture_matches_calendar_scoped_contributions() {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct DailyRecord {
            key: String,
            origin: String,
            start_time: i64,
            end_time: Option<i64>,
            capacity_end_time: Option<i64>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct DailyFixture {
            name: String,
            boundaries: Vec<i64>,
            sampled_at: i64,
            records: Vec<DailyRecord>,
            excluded_keys: Vec<String>,
            expected_daily: Vec<i64>,
        }
        let fixtures: Vec<DailyFixture> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/daily-activity-cases.json"
        ))
        .unwrap();
        for fixture in fixtures {
            let records: Vec<_> = fixture
                .records
                .into_iter()
                .map(|record| OwnedActivityRange {
                    origin: match record.origin.as_str() {
                        "native" => ActivityOrigin::Native,
                        "import_exact" => ActivityOrigin::ImportExact,
                        "import_bucket" => ActivityOrigin::ImportBucket,
                        value => panic!("unsupported fixture origin: {value}"),
                    },
                    start_ms: record.start_time,
                    end_ms: record.end_time.unwrap_or(fixture.sampled_at),
                    capacity_end_ms: record.capacity_end_time,
                    value: record.key,
                })
                .collect();
            let actual: Vec<i64> = fixture
                .boundaries
                .windows(2)
                .map(|day| {
                    summarize_activity_range(&records, day[0], day[1])
                        .into_iter()
                        .filter(|item| !fixture.excluded_keys.contains(&item.value))
                        .map(|item| item.duration_ms)
                        .sum()
                })
                .collect();
            assert_eq!(actual, fixture.expected_daily, "{}", fixture.name);
        }
    }
}
