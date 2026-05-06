/// V0 connection lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleState {
    /// Awaiting client `Hello` frame.
    Hello,
    /// `Hello` accepted; awaiting `Auth` frame.
    Auth,
    /// Authenticated session; RPC and contract frames may be dispatched.
    Active,
    /// Drain initiated; new commands are rejected, in-flight result streams
    /// may continue to completion.
    Draining,
    /// Session terminated. Any further frames are protocol errors.
    Closed,
}
