use std::env;
use std::path::PathBuf;

fn main() {
    let dst_triples = cmake::Config::new("triples")
        .define("CMAKE_BUILD_TYPE", "Release")
        .build();

    // Include the local library in the wrapper
    println!(
        "cargo:rustc-link-search=native={}",
        dst_triples.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        dst_triples.join("lib64").display()
    );

    // Link the wrapper library.
    println!("cargo:rustc-link-lib=dylib=wrapper");

    // Include dependencies.
    println!("cargo:rustc-link-search=native={}", "/usr/local/lib");
    println!("cargo:rustc-link-lib=wrapper");
    println!("cargo:rustc-link-lib=emp-tool");
    println!("cargo:rustc-link-lib=mpc");
    println!("cargo:rustc-link-lib=ssl");
    println!("cargo:rustc-link-lib=gmp");
    println!("cargo:rustc-link-lib=crypto");

    println!(
        "cargo:rustc-link-arg=-Wl,-rpath={}",
        dst_triples.join("lib").display()
    );

    // Re-run the build if some C/C++ code changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=triples/CMakeLists.txt");
    println!("cargo:rerun-if-changed=triples/src");
    println!("cargo:rerun-if-changed=triples/include");
    println!("cargo:rerun-if-changed=triples/internal");
}
