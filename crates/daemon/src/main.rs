#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fire_box_core::run_from_args().await
}
