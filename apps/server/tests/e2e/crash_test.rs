//! Kill the real binary at each failpoint and check no acknowledged event is
//! lost or stored twice. These pin today's guarantee so the ingest/digest
//! redesign cannot regress it.

use crate::common::crash::{assert_acked_events_survive, assert_alerts_survive};

#[tokio::test]
async fn crash_after_spool_fsync_before_link() {
    assert_acked_events_survive("ingest.after_fsync_before_link").await;
}

#[tokio::test]
async fn crash_after_spool_link_before_ack() {
    assert_acked_events_survive("ingest.after_link_before_ack").await;
}

#[tokio::test]
async fn crash_mid_digest_transaction() {
    assert_acked_events_survive("digest.mid_transaction").await;
}

#[tokio::test]
async fn crash_after_digest_commit_before_release() {
    assert_acked_events_survive("digest.after_commit_before_release").await;
}

#[tokio::test]
async fn crash_after_alerts_before_delete() {
    assert_acked_events_survive("digest.after_alerts_before_delete").await;
}

/// The alerts of a batch ran, then the process died before its files went:
/// the replay records nothing new and delivers nothing twice.
#[tokio::test]
async fn crash_after_alerts_before_delete_with_an_alert_rule() {
    assert_alerts_survive("digest.after_alerts_before_delete").await;
}
