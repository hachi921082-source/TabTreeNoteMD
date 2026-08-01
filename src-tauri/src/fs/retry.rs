//! Windows lock-error retry policy (`docs/spec.md` §5.7; `docs/detailed-spec.md`
//! §3.7; `docs/plans/persistence-fs/PLAN.md` §7).
//!
//! Used by `atomic_write`'s rename steps, `memo_delete`'s direct-delete
//! fallback, tab-lock's file rename/re-encrypt operations, and any other
//! write/rename/delete call site touching a possibly-locked file.

use std::io;
use std::thread::sleep;
use std::time::Duration;

/// 4 attempts total, 50ms/100ms/200ms/200ms backoff between them
/// (`docs/detailed-spec.md` §3.7).
const RETRY_BACKOFF: [Duration; 3] = [
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(200),
];
const MAX_ATTEMPTS: u32 = 4;

/// Runs `op`, retrying on a Windows sharing-violation/permission-class error
/// (`ErrorKind::PermissionDenied`, or the raw OS errors for
/// `ERROR_SHARING_VIOLATION` (32) / `ERROR_LOCK_VIOLATION` (33)) up to
/// `MAX_ATTEMPTS` times. On final failure, the last error is propagated; the
/// caller maps it to `ErrorCode::IoLocked` at the command boundary.
pub fn with_retry<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut attempt = 0;
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(err) if attempt < MAX_ATTEMPTS - 1 && is_lock_class_error(&err) => {
                sleep(RETRY_BACKOFF[attempt as usize]);
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Exposed beyond this module so a command handler that already caught a
/// `with_retry`-exhausted error can classify it the same way, to decide
/// between `ErrorCode::IoLocked` and a more generic error code
/// (`docs/plans/memo-tree/PLAN.md` §3 `memo_rename`/`memo_move`).
pub fn is_lock_class_error(err: &io::Error) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;

    if err.kind() == io::ErrorKind::PermissionDenied {
        return true;
    }
    matches!(err.raw_os_error(), Some(ERROR_SHARING_VIOLATION) | Some(ERROR_LOCK_VIOLATION))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn succeeds_immediately_when_op_succeeds() {
        let result = with_retry(|| Ok::<_, io::Error>(42));
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn retries_lock_class_errors_then_succeeds() {
        let attempts = Cell::new(0);
        let result = with_retry(|| {
            let n = attempts.get() + 1;
            attempts.set(n);
            if n < 3 {
                Err(io::Error::from_raw_os_error(32))
            } else {
                Ok(n)
            }
        });
        assert_eq!(result.unwrap(), 3);
        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn gives_up_after_max_attempts() {
        let attempts = Cell::new(0);
        let result = with_retry(|| {
            attempts.set(attempts.get() + 1);
            Err::<(), _>(io::Error::from_raw_os_error(32))
        });
        assert!(result.is_err());
        assert_eq!(attempts.get(), MAX_ATTEMPTS);
    }

    #[test]
    fn does_not_retry_non_lock_errors() {
        let attempts = Cell::new(0);
        let result = with_retry(|| {
            attempts.set(attempts.get() + 1);
            Err::<(), _>(io::Error::new(io::ErrorKind::NotFound, "gone"))
        });
        assert!(result.is_err());
        assert_eq!(attempts.get(), 1);
    }
}
