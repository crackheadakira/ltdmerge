use std::cell::RefCell;

pub trait Log {
    fn info(&self, msg: &str);
    fn warn(&self, msg: &str);
}

pub struct StdoutLog;

impl Log for StdoutLog {
    fn info(&self, msg: &str) {
        println!("{msg}");
    }

    fn warn(&self, msg: &str) {
        eprintln!("warning: {msg}");
    }
}

thread_local! {
    static SINK: RefCell<Option<Box<dyn Log>>> = const { RefCell::new(None) };
}

struct Restore(Option<Box<dyn Log>>);

impl Drop for Restore {
    fn drop(&mut self) {
        SINK.with(|sink| *sink.borrow_mut() = self.0.take());
    }
}

pub fn with_sink<R>(log: Box<dyn Log>, f: impl FnOnce() -> R) -> R {
    let _restore = Restore(SINK.with(|sink| sink.borrow_mut().replace(log)));

    f()
}

pub fn info(msg: &str) {
    SINK.with(|sink| match &*sink.borrow() {
        Some(log) => log.info(msg),
        None => StdoutLog.info(msg),
    })
}

pub fn warn(msg: &str) {
    SINK.with(|sink| match &*sink.borrow() {
        Some(log) => log.warn(msg),
        None => StdoutLog.warn(msg),
    })
}
