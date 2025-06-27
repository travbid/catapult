
#include <iostream>

#include "exmath.hpp"

int main(int argc, const char * argv[]) {
    std::cout << "Hello, World!\n";
    const auto added = add(argc, argc);
    const auto subbed = sub(added, argc/2);
    std::cout << added << " : " << subbed << "\n";
    return 0;
}
