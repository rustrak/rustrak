//! Files kept beside an event: today, the screenshots a feedback widget takes
//! (`event_attachments`, written by the digest).

use std::collections::HashMap;

use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::AttachmentInfo;

#[derive(sqlx::FromRow)]
struct IssueAttachment {
    issue_id: Uuid,
    #[sqlx(flatten)]
    info: AttachmentInfo,
}

const INFO_COLUMNS: &str = "a.id, a.event_id, a.filename, a.content_type, a.size, a.created_at";

pub struct AttachmentService;

impl AttachmentService {
    /// The attachments of one of a project's events.
    pub async fn list_for_event(
        pool: &DbPool,
        project_id: i32,
        event_id: Uuid,
    ) -> AppResult<Vec<AttachmentInfo>> {
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {INFO_COLUMNS} FROM event_attachments a \
             WHERE a.project_id = $1 AND a.event_id = $2 ORDER BY a.created_at, a.id"
        )))
        .bind(project_id)
        .bind(event_id)
        .fetch_all(pool)
        .await?)
    }

    /// The attachments of each issue's events, for a page of issues.
    pub async fn list_for_issues(
        pool: &DbPool,
        issue_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, Vec<AttachmentInfo>>> {
        let mut out: HashMap<Uuid, Vec<AttachmentInfo>> = HashMap::new();
        if issue_ids.is_empty() {
            return Ok(out);
        }
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(format!(
            "SELECT e.issue_id, {INFO_COLUMNS} FROM event_attachments a \
             JOIN events e ON e.id = a.event_id WHERE e.issue_id IN ("
        ));
        let mut ids = qb.separated(", ");
        for id in issue_ids {
            ids.push_bind(*id);
        }
        qb.push(") ORDER BY a.created_at, a.id");
        let rows: Vec<IssueAttachment> = qb.build_query_as().fetch_all(pool).await?;
        for row in rows {
            out.entry(row.issue_id).or_default().push(row.info);
        }
        Ok(out)
    }

    /// One of a project's attachments, with its bytes.
    pub async fn get(
        pool: &DbPool,
        project_id: i32,
        id: Uuid,
    ) -> AppResult<(AttachmentInfo, Vec<u8>)> {
        #[derive(sqlx::FromRow)]
        struct Row {
            #[sqlx(flatten)]
            info: AttachmentInfo,
            data: Vec<u8>,
        }
        let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {INFO_COLUMNS}, a.data FROM event_attachments a \
             WHERE a.project_id = $1 AND a.id = $2"
        )))
        .bind(project_id)
        .bind(id)
        .fetch_optional(pool)
        .await?;
        let row = row.ok_or_else(|| AppError::NotFound(format!("Attachment {id} not found")))?;
        Ok((row.info, row.data))
    }
}

/// The image type `data` really is, from its first bytes, for the four
/// formats a browser renders without running anything. The declared content
/// type is the sender's word; an SVG or HTML file claiming to be an image
/// would run script if served inline.
pub fn image_type(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::image_type;

    #[test]
    fn recognises_the_four_inline_image_formats() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(image_type(b"\xff\xd8\xff\xe0rest"), Some("image/jpeg"));
        assert_eq!(image_type(b"GIF89arest"), Some("image/gif"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
    }

    #[test]
    fn anything_else_is_not_an_image() {
        assert_eq!(
            image_type(b"<svg xmlns=\"http://www.w3.org/2000/svg\">"),
            None
        );
        assert_eq!(image_type(b"<html>"), None);
        assert_eq!(image_type(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(image_type(b""), None);
    }
}
