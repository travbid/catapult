#include "my_depend.hpp"

#include <iostream>

int main() {
  if (my_function(2, 3) != 5) {
    std::cout << "fail\n";
    return EXIT_FAILURE;
  }

  if (my_function(4, 5) != 9) {
    std::cout << "fail\n";
    return EXIT_FAILURE;
  }

  return EXIT_SUCCESS;
}
