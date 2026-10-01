fn main() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let script = format!("{dir}/linker-{arch}.ld");
    println!("cargo:rustc-link-arg=-T{script}");
    println!("cargo:rerun-if-changed={script}");
}
