use std::{collections::VecDeque, sync::Mutex};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{TimeDelta, Utc};

use super::EventCoordinator;
use crate::{
    domain::{SystemEvent, TimerEvent},
    event_source::EventSource,
};

struct GenericEventCoordinatorState {
    enqueued_events: VecDeque<SystemEvent>,
    last_timer_event: Option<chrono::DateTime<Utc>>,
}

pub struct GenericEventCoordinator {
    state: Mutex<GenericEventCoordinatorState>,
    event_source: Box<dyn EventSource>,
    frequency: TimeDelta,
}

impl GenericEventCoordinator {
    pub fn new(event_source: Box<dyn EventSource>, frequency: TimeDelta) -> Self {
        GenericEventCoordinator {
            state: Mutex::new(GenericEventCoordinatorState {
                enqueued_events: VecDeque::new(),
                last_timer_event: None,
            }),
            event_source,
            frequency,
        }
    }

    fn should_generate_timer_event(&self, state: &mut GenericEventCoordinatorState) -> bool {
        let now = Utc::now();
        if let Some(last_time) = state.last_timer_event {
            if now.signed_duration_since(last_time) >= self.frequency {
                state.last_timer_event = Some(now);
                return true;
            }
        } else {
            state.last_timer_event = Some(now);
            return true;
        }
        false
    }
}

#[async_trait]
impl EventCoordinator for GenericEventCoordinator {
    async fn enqueue_event(&self, event: SystemEvent) -> Result<()> {
        self.state.lock().unwrap().enqueued_events.push_back(event);
        Ok(())
    }

    async fn next_event(&self) -> Result<SystemEvent> {
        loop {
            // First, check if there are any enqueued events
            let enqueued_event = {
                let mut state = self.state.lock().unwrap();
                state.enqueued_events.pop_front()
            };

            if let Some(event) = enqueued_event {
                return Ok(event);
            }

            // Next, check if we should generate a timer event
            {
                let mut state = self.state.lock().unwrap();
                if self.should_generate_timer_event(&mut state) {
                    let timer_event = SystemEvent::Timer(TimerEvent {
                        timestamp: Utc::now(),
                    });
                    tracing::debug!("Generated timer event: {:?}", timer_event);
                    return Ok(timer_event);
                }
            }

            // Finally, try to get an event from the source
            if let Some(event) = self.event_source.next_event().await? {
                return Ok(event);
            }

            // If we got here, there was no event available from the source
            // Let's sleep a bit and try again
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}
