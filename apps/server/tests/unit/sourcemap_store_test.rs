use bytes::Bytes;
use rustrak::services::sourcemap_store::{LocalSourceMapStore, SourceMapStore, StoreError};
use tempfile::tempdir;

#[tokio::test]
async fn test_store_put_get_roundtrip() {
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());
    let data = Bytes::from_static(b"hello sourcemap");
    store
        .put("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", data.clone())
        .await
        .unwrap();
    let retrieved = store
        .get("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
        .await
        .unwrap();
    assert_eq!(retrieved, data);
}

#[tokio::test]
async fn test_store_put_idempotent() {
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());
    let key = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let original = Bytes::from_static(b"original content");
    let different = Bytes::from_static(b"different content");
    store.put(key, original.clone()).await.unwrap();
    store.put(key, different).await.unwrap(); // must not error
    assert_eq!(store.get(key).await.unwrap(), original);
}

#[tokio::test]
async fn test_store_get_missing() {
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());
    let result = store.get("0000000000000000000000000000000000000000").await;
    assert!(matches!(result, Err(StoreError::NotFound(_))));
}

#[tokio::test]
async fn test_store_exists_after_put() {
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());
    store
        .put(
            "ab12345678ab12345678ab12345678ab12345678",
            Bytes::from_static(b"data"),
        )
        .await
        .unwrap();
    assert!(store
        .exists("ab12345678ab12345678ab12345678ab12345678")
        .await
        .unwrap());
    assert!(!store
        .exists("1111111111111111111111111111111111111111")
        .await
        .unwrap());
}

#[tokio::test]
async fn test_store_delete_existing() {
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());
    store
        .put(
            "ab12345678ab12345678ab12345678ab12345678",
            Bytes::from_static(b"data"),
        )
        .await
        .unwrap();
    store
        .delete("ab12345678ab12345678ab12345678ab12345678")
        .await
        .unwrap();
    let result = store.get("ab12345678ab12345678ab12345678ab12345678").await;
    assert!(matches!(result, Err(StoreError::NotFound(_))));
}

#[tokio::test]
async fn test_store_delete_missing_noop() {
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());
    let result = store
        .delete("0000000000000000000000000000000000000001")
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_store_cas_layout() {
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());
    let key = "abcdef1234567890abcdef1234567890abcdef12";
    store.put(key, Bytes::from_static(b"data")).await.unwrap();
    let expected = tmp
        .path()
        .join("ab")
        .join("cdef1234567890abcdef1234567890abcdef12.map");
    assert!(
        expected.exists(),
        "CAS file not at expected path: {expected:?}"
    );
}

#[tokio::test]
async fn test_store_path_traversal_in_key() {
    let tmp = tempdir().unwrap();
    let store = LocalSourceMapStore::new(tmp.path());
    let result = store
        .put("../../etc/passwd", Bytes::from_static(b"bad"))
        .await;
    assert!(result.is_err(), "traversal key must be rejected");
    let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert!(
        entries.is_empty(),
        "store root must remain empty after rejected traversal"
    );
}
