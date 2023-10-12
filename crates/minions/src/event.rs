pub trait Event: Send + Sync + 'static {}

pub struct BoxedEvent(pub Box<dyn Any + Send + Sync>);

impl BoxedEvent {
    pub fn downcast<T: Event>(self) -> Result<T, BoxedEvent> {
        if self.0.is::<T>() {
            Ok(*self.0.downcast::<T>().unwrap())
        } else {
            Err(self)
        }
    }

    pub fn downcast_unchecked<T: Event>(self) -> T {
        *self.0.downcast_unchecked::<T>()
    }
}
