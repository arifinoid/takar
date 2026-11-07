//! System units. Reference data shared by every tenant.

use axum::{extract::State, Json};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::guard::{CanReadMaterials, Require};
use crate::{error::ApiError, AppState};

#[derive(Debug, Serialize)]
pub struct UnitRef {
    pub id: Uuid,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct UnitView {
    pub id: Uuid,
    pub code: String,
    pub name: String,
}

pub async fn list_units(
    State(state): State<AppState>,
    _guard: Require<CanReadMaterials>,
) -> Result<Json<Vec<UnitView>>, ApiError> {
    let db = state.db()?;

    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("select id, code, name from units where tenant_id is null order by name")
            .fetch_all(db)
            .await?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, code, name)| UnitView { id, code, name })
            .collect(),
    ))
}
