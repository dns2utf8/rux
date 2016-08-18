use poll::EpollEvent;
use error::Result;

pub mod sync;

pub trait Controller where Self: Sized {

    fn is_terminated(&self) -> bool;

    fn ready(&mut self, events: &EpollEvent) -> Result<()>;

}
