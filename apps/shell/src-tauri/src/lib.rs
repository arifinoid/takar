use tauri_plugin_sql::{Migration, MigrationKind};

/// Local queue for stock receipts recorded while offline.
///
/// The payload is the request body the shell would have sent, and
/// `client_ref` is the idempotency key the server uses to recognise a replay.
fn migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        description: "stock outbox",
        sql: "create table stock_outbox (
                client_ref text primary key,
                payload text not null,
                status text not null default 'pending',
                code text,
                attempts integer not null default 0,
                created_at text not null default (datetime('now'))
              );",
        kind: MigrationKind::Up,
    }]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:takar.db", migrations())
                .build(),
        )
        .run(tauri::generate_context!())
        .expect("error while running the Tauri application");
}
