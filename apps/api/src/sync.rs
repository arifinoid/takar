//! Offline sync.
//!
//! The Tauri shell records stock receipts locally while offline and replays
//! them after reconnecting. Each entry carries a client reference, so sending
//! the same queue twice never duplicates a batch, and entries are independent:
//! one rejected receipt does not roll back the others.

use axum::{extract::State, routing::post, Json, Router};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::guard::{CanWriteBatches, Require};
use crate::stock::batches::{insert_stock_in, StockInBody};
use crate::{error::ApiError, AppState};

/// A queue longer than this is a client bug, not a real offline day.
const MAX_ENTRIES: usize = 200;

#[derive(Debug, serde::Deserialize)]
pub struct SyncBody {
    pub entries: Vec<StockInBody>,
}

#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub client_ref: Option<Uuid>,
    /// `created`, `duplicate`, or `rejected`.
    pub status: &'static str,
    pub batch_id: Option<Uuid>,
    /// The API error code when the entry was rejected.
    pub code: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub results: Vec<SyncResult>,
}

pub async fn sync_stock_in(
    State(state): State<AppState>,
    guard: Require<CanWriteBatches>,
    Json(body): Json<SyncBody>,
) -> Result<Json<SyncResponse>, ApiError> {
    if body.entries.len() > MAX_ENTRIES {
        return Err(ApiError::BadRequest("too_many_entries"));
    }

    let auth = guard.auth;
    let db = state.db()?;
    let mut results = Vec::with_capacity(body.entries.len());

    for entry in body.entries {
        let client_ref = entry.client_ref;

        // Without a reference the server cannot tell a retry from a second
        // delivery, so the entry is refused instead of risking a duplicate.
        if client_ref.is_none() {
            results.push(SyncResult {
                client_ref,
                status: "rejected",
                batch_id: None,
                code: Some("client_ref_required"),
            });
            continue;
        }

        let mut tx = takar_db::begin_scoped(db, Some(auth.tenant_id), Some(auth.user_id)).await?;

        match insert_stock_in(&mut tx, auth.tenant_id, auth.user_id, &entry).await {
            Ok(record) => {
                let status = if record.created {
                    "created"
                } else {
                    "duplicate"
                };
                tx.commit().await?;
                results.push(SyncResult {
                    client_ref,
                    status,
                    batch_id: Some(record.batch.id),
                    code: None,
                });
            }
            Err(error) => {
                tx.rollback().await?;
                results.push(SyncResult {
                    client_ref,
                    status: "rejected",
                    batch_id: None,
                    code: Some(error.code()),
                });
            }
        }
    }

    Ok(Json(SyncResponse { results }))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/sync/stock-in", post(sync_stock_in))
}
