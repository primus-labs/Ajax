# Rust bindings to EMP-tool

## Requirements

To make the bindings work, you need the libraries listed in the `build.rs` file installed in your system:

- OpenSSL,
- [LibGMP](https://gmplib.org/),
- LibCrypto,
- EMP-tool: run the command `python install.py --deps --tool --ot` as explained in
  the [EMP-tool README](https://github.com/emp-toolkit/emp-tool).
- Build the library wrapper in `thfhe/triples` folder: To install this library, you can use the `CMakeLists.txt` file in
  the mentioned folder. Once you have built the library, you must change the `build.rs` file according to the directory
  of your built library file. Specifically, you must set the following line according to the build folder:
  ```rust
  println!("cargo:rustc-link-search=native=thfhe/triples/cmake-build-release");
  ```
  In the previous example, the built `libwrapper.so` file (that was built using the `CMakeLists.txt`) is stored in the
  folder `/thfhe/triples/cmake-build-release/`.