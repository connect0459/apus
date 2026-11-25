pub const STACK_SIZE: usize = 256 << 20;
pub const MAX_NESTING: usize = 256;
pub const MAX_DEPTH: usize = 8000;

pub fn run_with_large_stack<T: Send>(work: impl FnOnce() -> T + Send) -> Result<T, String> {
    std::thread::scope(|scope| {
        let spawned = std::thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn_scoped(scope, work);
        match spawned {
            Ok(handle) => Ok(handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))),
            Err(error) => Err(format!(
                "cannot start a thread with a stack of {STACK_SIZE} bytes: {error}"
            )),
        }
    })
}
