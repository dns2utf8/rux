use std::os::unix::io::RawFd;
use std::cell::RefCell;
use std::boxed::Box;

use nix::unistd;
use slab::Slab;

use error::Error;
use handler::*;
use protocol::{IOProtocol, Action};
use poll::*;

// TODO should take whether epoll mode is ET or LT (as marker traits)
pub struct SyncHandler<F: IOProtocol> {
    epfd: EpollFd,
    handlers: Slab<RefCell<Box<Handler<EpollEvent>>>, usize>,
    eproto: F,
    rproto: F::Protocol,
}

impl<F: IOProtocol> SyncHandler<F> {
    pub fn new(epfd: EpollFd,
               eproto: F,
               rproto: F::Protocol,
               max_handlers: usize)
               -> SyncHandler<F> {
        trace!("new()");
        SyncHandler {
            epfd: epfd,
            handlers: Slab::new(max_handlers),
            eproto: eproto,
            rproto: rproto,
        }
    }

    fn notify(&mut self, fd: RawFd, id: usize, event: &EpollEvent) {
        trace!("notify()");

        let events = event.events;

        if events.contains(EPOLLRDHUP) || events.contains(EPOLLHUP) {

            trace!("socket's fd {}: EPOLLHUP", fd);
            match self.handlers.remove(id) {
                None => error!("on_close(): handler not found"),
                Some(_) => {}
            }
            perror!("unregister()", self.epfd.unregister(fd));
            perror!("close()", unistd::close(fd));
            debug!("handlers: {:?}", self.handlers);
        } else {
            let handler = &mut self.handlers[id];
            handler.borrow_mut().ready(event);
        }
    }
}

impl<F: IOProtocol> Handler<EpollEvent> for SyncHandler<F> {

    fn is_terminated(&self) -> bool {
        false
    }

    fn ready(&mut self, ev: &EpollEvent) {
        trace!("ready()");

        match self.eproto.decode(ev.data) {

            Action::New(proto, fd) => {
                // TODO a little bit arbitrary; protocol should provide methods for next
                // handler in the stack
                let next = if proto.into() == 0_usize { self.rproto } else { proto };
                trace!("ready(): new handler ({})", next.into());

                if let Ok(id) = self.handlers
                    .insert(RefCell::new(self.eproto.get_handler(next, self.epfd))) {

                    let action: Action<F> = Action::Notify(id, fd);

                    let interest = EpollEvent {
                        events: EPOLLIN | EPOLLOUT | EPOLLHUP | EPOLLRDHUP,
                        data: self.eproto.encode(action),
                    };

                    match self.epfd.reregister(fd, &interest) {
                        Ok(_) => self.notify(fd, id, ev),
                        Err(e) => report_err!("reregister()", e),
                    }
                } else {
                    // TODO handle too many handlers
                    // .map_err(|_| "reached maximum number of handlers") {
                }
            }

            // TODO handler should abstract over a type parameter
            // so that this handler takes Handlers of Action
            // and we don't have to decode the event twice
            Action::Notify(id, fd) => self.notify(fd, id, ev),
        }
    }
}
