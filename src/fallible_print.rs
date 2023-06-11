pub use anyhow::anyhow;
pub use std::io::Write;

// Fallible println! and print! macros, so we can catch and ignore io::ErrorKind::BrokenPipe.
// Adapted from: https://github.com/rust-lang/rust/issues/46016#issuecomment-1242039016
#[macro_export]
macro_rules! println {
    () => (print!("\n"));
    ($fmt:expr) => ({
        writeln!(std::io::stdout(), $fmt).map_err(|e| anyhow!(e))
    });
    ($fmt:expr, $($arg:tt)*) => ({
        writeln!(std::io::stdout(), $fmt, $($arg)*).map_err(|e| anyhow!(e))
    })
}

#[macro_export]
macro_rules! print {
    () => (print!("\n"));
    ($fmt:expr) => ({
        write!(std::io::stdout(), $fmt).map_err(|e| anyhow!(e))
    });
    ($fmt:expr, $($arg:tt)*) => ({
        write!(std::io::stdout(), $fmt, $($arg)*).map_err(|e| anyhow!(e))
    })
}

#[macro_export]
macro_rules! eprintln {
    () => (print!("\n"));
    ($fmt:expr) => ({
        writeln!(std::io::stderr(), $fmt).map_err(|e| anyhow!(e))
    });
    ($fmt:expr, $($arg:tt)*) => ({
        writeln!(std::io::stderr(), $fmt, $($arg)*).map_err(|e| anyhow!(e))
    })
}
