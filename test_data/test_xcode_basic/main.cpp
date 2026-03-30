#include "add.hpp"

#include <iostream>

int main(int argc, char**) {
    std::cout << "Hello, world\n";
    return add(argc, argc) - 2;
}
