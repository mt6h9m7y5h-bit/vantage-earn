//! Apply embedded migrations without starting the HTTP server.
//! Used by scripts/db-migrate.sh when sqlx-cli is not installed.

use api_gateway::store::normalize_database_url;

#[tokio::main]
async fn main() {
    let url = normalize_database_url(
        &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
    );
    let pool = sqlx::PgPool::connect(&url)
        .await
        .expect("failed to connect to PostgreSQL");
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("migration failed");
    println!("Migrations applied successfully.");
}
