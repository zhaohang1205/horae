-- Normalize bare-letter rrule shorthand stored in older versions.
-- `*d`/`*w`/`*m`/`*y` used to be stored as-is (e.g. rrule = 'd'); these were
-- silently interpreted as daily by rrule_occurrences. Rewrite them to the
-- equivalent full RRULE so stored values are valid and self-documenting.
UPDATE tasks
SET rrule = CASE rrule
    WHEN 'd' THEN 'FREQ=DAILY'
    WHEN 'w' THEN 'FREQ=WEEKLY'
    WHEN 'm' THEN 'FREQ=MONTHLY'
    WHEN 'y' THEN 'FREQ=YEARLY'
    ELSE rrule
END
WHERE rrule IN ('d', 'w', 'm', 'y');
