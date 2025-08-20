fn main() {
    println!("cargo:rustc-link-search=native=emp-bindings/lib");
    println!("cargo:rustc-link-lib=wrapper");
}
