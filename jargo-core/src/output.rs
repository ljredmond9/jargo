/// Print a verbose-only message. No-op unless `-v`/`--verbose` was passed.
#[macro_export]
macro_rules! vprintln {
    ($gctx:expr, $($arg:tt)*) => {
        if $gctx.verbose {
            println!($($arg)*);
        }
    };
}
