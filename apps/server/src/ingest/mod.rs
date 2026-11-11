pub mod decompression;
pub mod envelope;
pub mod parser;
pub mod pipeline;
pub mod storage;

pub use decompression::{read_body, MAX_COMPRESSED_SIZE};
pub use envelope::{EnvelopeItemKind, EventMetadata};

/// The largest attachment kept. A larger one is dropped on its own and its
/// event kept. ponytail: attachments live in the database, so this also
/// bounds a row; move them to a file store before raising it much.
pub const MAX_ATTACHMENT_SIZE: usize = 10 * 1024 * 1024;
pub(crate) use parser::detach_payload_if_needed;
pub use parser::EnvelopeParser;
pub use storage::{
    delete_event, list_pending_event_metadata, prepare_ingest_dir, read_event,
    store_event_with_metadata,
};
