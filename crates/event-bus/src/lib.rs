use std::sync::{Arc, RwLock};

use shared::AppEvent;
use tokio::sync::broadcast;

type Handler = Arc<dyn Fn(&AppEvent) + Send + Sync>;

/// In-process event bus. Replace with Kafka in Phase 3.
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
    handlers: RwLock<Vec<Handler>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            tx,
            handlers: RwLock::new(Vec::new()),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }

    pub fn on<F>(&self, handler: F)
    where
        F: Fn(&AppEvent) + Send + Sync + 'static,
    {
        self.handlers.write().unwrap().push(Arc::new(handler));
    }

    pub async fn publish(&self, event: AppEvent) {
        tracing::info!(?event, "domain event published");
        for handler in self.handlers.read().unwrap().iter() {
            handler(&event);
        }
        let _ = self.tx.send(event);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use shared::WatchCompletedPayload;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    #[tokio::test]
    async fn publishes_to_handlers() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        bus.on(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish(AppEvent::WatchCompleted(WatchCompletedPayload {
            user_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            watch_duration_secs: 30,
            reward_usdt: Decimal::new(1, 3),
            occurred_at: Utc::now(),
        }))
        .await;

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
