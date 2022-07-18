#include <iostream>

extern "C" void hello_cpp() {
    std::cout << "Hello C++ from Rust!" << std::endl;
}
