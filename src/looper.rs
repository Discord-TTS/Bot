#[poise::serenity_prelude::async_trait]
pub trait Looper {
    const NAME: &'static str;
    const MILLIS: u64;

    async fn loop_func(&self) -> anyhow::Result<()>;
    async fn start(self: std::sync::Arc<Self>) where Self: Sync {
        tracing::info!("{}: Started background task", Self::NAME);
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(Self::MILLIS));
        loop {
            interval.tick().await;
            if let Err(err) = self.loop_func().await {
                tracing::error!("{} Error: {:?}", Self::NAME, err);
            }
        }
    }
}
