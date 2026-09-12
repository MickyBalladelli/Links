use links_server_store::postgres::RelationalStore;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must name the intended Links database")?;
    let store = RelationalStore::connect(&url).await?;
    store.migrate().await?;
    store.close().await;
    println!("Links relational migrations applied.");
    Ok(())
}
