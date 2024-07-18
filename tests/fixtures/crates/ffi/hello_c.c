#include <stdio.h>

void hello_c() {
    if (1) {
        printf("Hello C from Rust!\n");
    } else {
        printf("this line in C is not covered\n");
    }
}
