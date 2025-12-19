use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let lib_path = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .join("thfhe/triples/build");

    // Include the local library in the wrapper
    println!("cargo:rustc-link-search=native={}", lib_path.display());
    println!("cargo:rustc-link-search=native=/usr/local/lib");
    println!("cargo:rustc-link-lib=wrapper");
    println!("cargo:rustc-link-lib=emp-tool");
    println!("cargo:rustc-link-lib=mpc");
    println!("cargo:rustc-link-lib=ssl");
    println!("cargo:rustc-link-lib=gmp");
    println!("cargo:rustc-link-lib=crypto");

    println!("cargo:rustc-link-arg=-Wl,-rpath={}", lib_path.display());
}
