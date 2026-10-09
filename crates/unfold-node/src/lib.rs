//! The Node-API addon that the Electron engine host loads (contract §3.1, architecture §12). It
//! exposes `init`, `subscribe`, `dispatch`, `cancel`, `detachWindow`, and `shutdown` over one
//! process-wide [`Host`], and holds no graph logic of its own.
//!
//! - Every value crossing the boundary is a JSON string (or a number for window and request IDs).
//! - The promise-returning functions start their work on a Rust thread (`Host::*_async`) and
//!   settle the promise from there through a thread-safe deferred, so the JavaScript thread never
//!   waits on Rust.
//! - A failure rejects with an `Error` whose message is the JSON `{"code","message"}` of a
//!   [`CommandError`]. A panic, in a command or in this layer, becomes an `internal` rejection.
//! - The sink is a weak (unref'd) thread-safe function called without blocking, so Rust threads
//!   never wait on JavaScript and the sink never keeps the process alive.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use napi::bindgen_prelude::{Function, Object, ToNapiValue};
use napi::threadsafe_function::ThreadsafeFunctionCallMode;
use napi::{Env, Error, JsDeferred, Result};
use napi_derive::napi;
use unfold_workspace::{CommandError, CommandResult, Host};

/// The one host of this process: the workspace and its projects live behind it.
static HOST: LazyLock<Host> = LazyLock::new(Host::new);

/// How a deferred promise gets its value on the JavaScript thread.
type Resolver<T> = Box<dyn FnOnce(Env) -> Result<T>>;

/// Called once, from any thread, with a request's outcome.
type Done<T> = Box<dyn FnOnce(CommandResult<T>) + Send>;

/// Sets the app folders from the JSON `{"appDataDir","logsDir","fallbackCacheDir"}` the main
/// process chose. Call once, before anything else; throws on bad options or a second call.
#[napi(catch_unwind)]
pub fn init(options: String) -> Result<()> {
    HOST.init(&options).map_err(|error| rejection(&error))
}

/// Registers the single sink. Rust calls it from any thread with one JSON message: an event for
/// every window or for some, or a stream message of a running command. Replaces any earlier sink.
#[napi(catch_unwind, ts_args_type = "sink: (message: string) => void")]
pub fn subscribe(sink: Function<String, ()>) -> Result<()> {
    let sink = sink
        .build_threadsafe_function::<String>()
        .weak::<true>()
        .build()?;
    HOST.subscribe(move |message| {
        // A full queue or a closing environment drops the message: the sink must never block a
        // Rust thread, and a message for a process that is exiting has no reader anyway.
        let _status = sink.call(message, ThreadsafeFunctionCallMode::NonBlocking);
    });
    Ok(())
}

/// Runs one command (`{"command","args"}` JSON) for one window (any window id: windows need no
/// attach). Resolves with the result JSON; stream messages go to the sink first. `requestId` must
/// be unique among running requests.
#[napi(catch_unwind, ts_return_type = "Promise<string>")]
pub fn dispatch(env: &Env, window_id: u32, request_id: i64, command: String) -> Result<Object<'_>> {
    let request = request_number(request_id);
    promise(env, move |done: Done<String>| match request {
        Ok(request) => HOST.dispatch_async(window_id, request, command, done),
        Err(error) => done(Err(error)),
    })
}

/// Cancels a running command or stream; does nothing when it already finished.
#[napi(catch_unwind)]
pub fn cancel(request_id: i64) {
    if let Ok(request) = request_number(request_id) {
        HOST.cancel(request);
    }
}

/// A window closed: waits (at most 15 s) for its closing writes in flight, then releases its board
/// claims and sessions and its Home selection.
#[napi(catch_unwind, ts_return_type = "Promise<void>")]
pub fn detach_window(env: &Env, window_id: u32) -> Result<Object<'_>> {
    promise(env, move |done| {
        HOST.detach_window_async(window_id, done);
    })
}

/// The app is quitting: closes every live project, remembering each one's state. Called once.
#[napi(catch_unwind, ts_return_type = "Promise<void>")]
pub fn shutdown(env: &Env) -> Result<Object<'_>> {
    promise(env, move |done| {
        HOST.shutdown_async(done);
    })
}

/// A request ID from JavaScript: a non-negative safe integer.
fn request_number(request_id: i64) -> CommandResult<u64> {
    u64::try_from(request_id)
        .map_err(|_| CommandError::invalid_args(format!("invalid request id {request_id}")))
}

/// The `Error` a failed call rejects (or throws) with: its message is the error's JSON.
fn rejection(error: &CommandError) -> Error {
    Error::from_reason(error.to_json())
}

/// Creates a promise and hands `start` the callback that settles it. The callback may run on any
/// thread. If `start` panics before handing the request to Rust, the promise rejects with
/// `internal` instead of staying pending.
fn promise<T, F>(env: &Env, start: F) -> Result<Object<'_>>
where
    T: ToNapiValue + Send + 'static,
    F: FnOnce(Done<T>),
{
    let (deferred, promise) = env.create_deferred::<T, Resolver<T>>()?;
    let slot = Arc::new(Mutex::new(Some(deferred)));
    let settle_slot = Arc::clone(&slot);
    let done: Done<T> = Box::new(move |result| {
        if let Some(deferred) = take(&settle_slot) {
            settle(deferred, result);
        }
    });
    let started = catch_unwind(AssertUnwindSafe(move || start(done)));
    if started.is_err()
        && let Some(deferred) = take(&slot)
    {
        settle(
            deferred,
            Err(CommandError::internal("the addon panicked starting the request")),
        );
    }
    Ok(promise)
}

fn take<T>(slot: &Mutex<Option<T>>) -> Option<T> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).take()
}

/// Resolves or rejects a deferred promise. Callable from any thread.
fn settle<T: ToNapiValue + Send + 'static>(deferred: JsDeferred<T, Resolver<T>>, result: CommandResult<T>) {
    match result {
        Ok(value) => deferred.resolve(Box::new(move |_env| Ok(value))),
        Err(error) => deferred.reject(rejection(&error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ids_must_be_non_negative() {
        assert_eq!(request_number(7).ok(), Some(7));
        let error = request_number(-1).expect_err("negative");
        assert_eq!(error.code, "invalid_args");
    }

    #[test]
    fn rejections_carry_the_error_json() {
        let error = rejection(&CommandError::new("not_compiled", "Compile first."));
        assert_eq!(
            error.reason,
            r#"{"code":"not_compiled","message":"Compile first."}"#
        );
    }
}
