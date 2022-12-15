fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=hello_c.c");
    println!("cargo:rerun-if-changed=hello_cpp.cpp");

    cc::Build::new().file("hello_c.c").compile("hello_c");
    cc::Build::new().cpp(true).file("hello_cpp.cpp").compile("libhello_cpp.a");
}
