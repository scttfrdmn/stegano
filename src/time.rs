//! Wall-clock timer for the embed/extract statistics that also works on
//! `wasm32-unknown-unknown`.
//!
//! `std::time::Instant::now()` panics there ("time not implemented on this
//! platform"), which aborted every embed in the WebAssembly bindings. On native
//! targets this is simply `std::time::Instant`; on wasm32 it reads
//! `Date.now()` through `js-sys` when the `wasm` feature is on, and reports
//! zero elapsed time otherwise. Only `now()` and `elapsed()` are provided —
//! the timings are informational (`performance_stats`), never load-bearing.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub(crate) use wasm_instant::Instant;

#[cfg(target_arch = "wasm32")]
mod wasm_instant {
    use std::time::Duration;

    /// Milliseconds since the Unix epoch (or `0.0` without a JS host).
    fn now_ms() -> f64 {
        #[cfg(feature = "wasm")]
        let ms = js_sys::Date::now();
        #[cfg(not(feature = "wasm"))]
        let ms = 0.0;
        ms
    }

    /// Minimal stand-in for `std::time::Instant` on wasm32.
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Instant(f64);

    impl Instant {
        pub(crate) fn now() -> Self {
            Instant(now_ms())
        }

        pub(crate) fn elapsed(&self) -> Duration {
            Duration::from_secs_f64((now_ms() - self.0).max(0.0) / 1000.0)
        }
    }
}
