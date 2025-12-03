fn main() {
    // Include the local library to the wrapper
    println!("cargo:rustc-link-search=native=thfhe/triples/build");
    println!("cargo:rustc-link-search=native=/usr/local/lib");
    println!("cargo:rustc-link-lib=wrapper");
    println!("cargo:rustc-link-lib=emp-tool");
    println!("cargo:rustc-link-lib=mpc");
    println!("cargo:rustc-link-lib=ssl");
    println!("cargo:rustc-link-lib=gmp");
    println!("cargo:rustc-link-lib=crypto");

    println!("cargo:rustc-link-arg=-Wl,-rpath=$ORIGIN/../../../thfhe/triples/build");
}
