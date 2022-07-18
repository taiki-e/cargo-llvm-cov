fn main() {
    cc::Build::new().file("hello_c.c").compile("hello_c");
    cc::Build::new().cpp(true).file("hello_cpp.cpp").compile("libhello_cpp.a");
}
