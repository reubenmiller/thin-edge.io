fn main() {
    // export GIT_SEMVER=$(git describe --always --tags --abbrev=8 --dirty)
    // https://github.com/rust-lang/cargo/issues/6583#issuecomment-1259871885
    if let Ok(val) = std::env::var("GIT_SEMVER") {
        println!("Using version defined by 'GIT_SEMVER={}'", val);
        println!("cargo:rustc-env=CARGO_PKG_VERSION={}", val);
    }
    println!("cargo:rerun-if-env-changed=GIT_SEMVER");
    println!("cargo:rerun-if-changed=build.rs");

    // Windows defaults the main thread stack to 1 MB; Linux is 8 MB.
    // The clap command tree built by TEdgeOptMulticall::command() is deep
    // enough to overflow the smaller stack before any user code runs,
    // causing "thread 'main' has overflowed its stack" on startup.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo:rustc-link-arg=/STACK:8388608"),
            Ok("gnu") => println!("cargo:rustc-link-arg=-Wl,--stack,8388608"),
            _ => {}
        }
    }
}
