Problems that occurred when trying to compile `thin-edge` on Windows:

1. Crate `notify` in `tedge_utils` uses `INotifyWatcher`, which is incompatible with Windows. Maybe switch to one more cross-system compatible?

2. Usage of `nix` and `std::os::unix` crates that are incompatible with Windows. However, they are primarily used in functions/methods that are not needed on Windows (user/group creation, setting permissions)

3. Relative/absolute paths provided by `Thin-edge` are incompatible with Windows.

4. Problem with handling root certificate for `c8y`.

5. Part of the `Tokio` crate is not compatible with Windows (e.g. `tokio::net::UnixStream` or `signal-hook-tokio`)