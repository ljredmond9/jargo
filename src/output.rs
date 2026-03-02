use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(v: bool) {
    VERBOSE.store(v, Ordering::Relaxed);
}

pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// Print a verbose-only message. No-op unless `-v`/`--verbose` was passed.
#[macro_export]
macro_rules! vprintln {
    ($($arg:tt)*) => {
        if $crate::output::is_verbose() {
            println!($($arg)*);
        }
    };
}
