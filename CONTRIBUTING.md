# Contributing

Thanks for helping improve SeeUsage.

1. Open an issue before large behavioral or dependency changes.
2. Keep the application tray-only and avoid webviews or background services.
3. Never commit credentials, raw account responses, or user-specific paths.
4. Add sanitized fixtures for provider payload changes.
5. Run formatting, Clippy, and all tests before opening a pull request.

```console
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Contributions are accepted under the repository's MIT License.
