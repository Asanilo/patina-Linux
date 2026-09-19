SELECT id, 'native' AS origin, exe_name, COALESCE(app_name, '') AS app_name,
       start_time, COALESCE(end_time, ?) AS end_time,
       COALESCE(end_time, ?) AS capacity_end_time
FROM sessions WHERE start_time < ? AND COALESCE(end_time, ?) > ?
UNION ALL
SELECT id, 'import_exact', exe_name, app_name, start_time, end_time, end_time
FROM import_exact_sessions WHERE start_time < ? AND end_time > ?
UNION ALL
SELECT id, 'import_bucket', exe_name, app_name, bucket_start_time,
       bucket_start_time + duration, bucket_start_time + 3600000
FROM import_time_buckets
WHERE bucket_start_time < ? AND bucket_start_time + 3600000 > ?
