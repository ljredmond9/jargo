#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Verbose,
    Normal,
    Quiet,
}

pub struct Shell {
    verbosity: Verbosity,
}

impl Shell {
    pub fn new(verbosity: Verbosity) -> Self {
        Shell { verbosity }
    }

    /// Cargo-style right-aligned status line: "{:>12} {message}"
    /// e.g. status("Compiling", "foo v1.0") → "   Compiling foo v1.0"
    /// Silent in Quiet mode.
    pub fn status(&self, verb: &str, message: &str) {
        if self.verbosity != Verbosity::Quiet {
            println!("{:>12} {}", verb, message);
        }
    }

    /// Execute a closure only in Verbose mode. The closure is never called
    /// (and no formatting happens) on the non-verbose path. Mirrors Cargo's pattern:
    ///
    ///   gctx.shell.verbose(|sh| sh.status("Fetching", "group:artifact:1.0"));
    ///
    /// Infallible (no Result) since jargo uses plain println! rather than a
    /// colored terminal writer that can fail.
    ///
    /// Why a closure instead of `verbose(impl Display)`:
    /// - Zero allocation on the non-verbose path (format strings never evaluated)
    /// - Inside the closure, `sh.status()` and other Shell methods are available,
    ///   letting verbose messages reuse the same structured formatting as normal output
    pub fn verbose<F: FnOnce(&Shell)>(&self, f: F) {
        if self.verbosity == Verbosity::Verbose {
            f(self);
        }
    }

    /// Print an unformatted line. Primarily used inside verbose() closures for
    /// diagnostic messages that don't fit the verb/message status pattern.
    pub fn print(&self, message: impl std::fmt::Display) {
        println!("{}", message);
    }

    pub fn warn(&self, message: &str) {
        if self.verbosity != Verbosity::Quiet {
            eprintln!("warning: {}", message);
        }
    }
}
