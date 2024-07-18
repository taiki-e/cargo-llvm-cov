#include <iostream>

extern "C" void hello_cpp() {
    if (1) {
        std::cout << "Hello C++ from Rust!" << std::endl;
    } else {
        std::cout << "this line in C++ is not covered" << std::endl;
    }
}
