UPDATE session_counts SET errored = GREATEST(errored - crashed - abnormal, 0);
DROP TABLE session_errors;
