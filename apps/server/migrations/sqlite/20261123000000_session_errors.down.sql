UPDATE session_counts SET errored = MAX(errored - crashed - abnormal, 0);
DROP TABLE session_errors;
