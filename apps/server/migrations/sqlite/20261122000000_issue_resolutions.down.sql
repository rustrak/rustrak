UPDATE issues SET status_details = '{"in_next_release":true}'
WHERE id IN (SELECT issue_id FROM issue_resolutions WHERE status = 0);
UPDATE issues SET status_details = '{}'
WHERE status_details LIKE '%inRelease%' OR status_details LIKE '%inNextRelease%';
DROP TABLE issue_resolutions;
