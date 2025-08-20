use std::env;
use std::path::PathBuf;

fn main() {
    let lib_dir_path = PathBuf::from("../thfhe/triples/ole")
        .canonicalize()
        .expect("could not canonicalize the path");

    let header_path = lib_dir_path.join("wrapper.hpp");
    let header_path_str = header_path.to_str().expect("invalid header path");

    println!("cargo:rustc-link-search={}", lib_dir_path.to_str().unwrap());
    println!("cargo:rustc-link-lib=wrapper");

    let bindings = bindgen::Builder::default()
        .header(header_path_str)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");
    bindings
        .write_to_file(out_path)
        .expect("couldn't write bindings!");
}
