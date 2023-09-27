use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use std::any::{Any, TypeId};

use async_trait::async_trait;
use tokio::task::JoinHandle;

#[async_trait]
pub trait Event: Send + Sync {
    async fn handle(&self, ox_frame: &OxFrame) -> Option<Vec<Box<dyn Event>>>;
}

#[derive(Clone)]
pub struct OxFrame {
    inner: Arc<Mutex<OxFrameInner>>,
}

pub struct OxFrameInner {
    event_bus: EventBus,
    context: HashMap<TypeId, Box<dyn Any + Send + 'static>>,
}

pub struct EventBus {
    tx: tokio::sync::mpsc::UnboundedSender<Box<dyn Event>>,
    rx: Option<tokio::sync::mpsc::UnboundedReceiver<Box<dyn Event>>>,
}

impl Default for OxFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl OxFrame {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let event_bus = EventBus { tx, rx: Some(rx) };
        OxFrame {
            inner: Arc::new(Mutex::new(OxFrameInner {
                event_bus,
                context: HashMap::new(),
            })),
        }
    }

    pub fn dispatch(&self, event: impl Event + 'static) {
        self.dispatch_boxed(Box::new(event));
    }

    fn dispatch_boxed(&self, event: Box<dyn Event>) {
        self.inner.lock().unwrap().event_bus.tx.send(event).unwrap();
    }

    pub fn provide_context<T: Any + Send + 'static>(&self, value: T) {
        self.inner
            .lock()
            .unwrap()
            .context
            .insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn use_context<T: Any + Send + Clone + 'static>(&self) -> Option<T> {
        let context = &self.inner.lock().unwrap().context;
        context
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
    }

    pub fn with_context<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Any + Send + 'static,
        F: FnOnce(&T) -> R,
    {
        let context = &self.inner.lock().unwrap().context;
        context
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>().map(f))
    }

    pub fn with_context_mut<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Any + Send + 'static,
        F: FnOnce(&mut T) -> R,
    {
        let context = &mut self.inner.lock().unwrap().context;
        context
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut::<T>().map(f))
    }
}

pub async fn run(ox_frame: &OxFrame) -> JoinHandle<()> {
    let ox_clone = ox_frame.clone();
    tokio::spawn(async move {
        let mut rx = ox_clone.inner.lock().unwrap().event_bus.rx.take().unwrap();
        while let Some(event) = rx.recv().await {
            if let Some(events) = event.handle(&ox_clone).await {
                for event in events {
                    ox_clone.dispatch_boxed(event);
                }
            }
        }
    })
}

pub struct TestEvent {}

#[async_trait]
impl Event for TestEvent {
    async fn handle(&self, ox_frame: &OxFrame) -> Option<Vec<Box<dyn Event>>> {
        println!("handle test event");
        let foo = ox_frame.use_context::<String>();
        dbg!(foo);
        None
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn it_works() {
        let ox_frame = OxFrame::new();
        run(&ox_frame).await;
        ox_frame.provide_context::<String>("bar".to_string());
        let test_event = TestEvent {};
        ox_frame.dispatch(test_event);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
