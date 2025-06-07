#![allow(async_fn_in_trait)]
#![feature(never_type, trait_alias)]

mod analytics;
pub mod bot_list_updater;
pub mod logging;
pub mod web_updater;

pub trait Looper {
    const NAME: &'static str;
    const MILLIS: u64;

    type Error: std::fmt::Debug;

    async fn loop_func(&self) -> Result<(), Self::Error>;
    async fn start(self)
    where
        Self: Sized,
    {
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
