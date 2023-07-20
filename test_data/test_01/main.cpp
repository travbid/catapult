#include <cstdlib>
#include <iostream>

#include "mylib.hpp"

int main(int argc, char** argv) {  //
  const auto ret = add_two(argc);
  std::cout << "Returning " << ret << "\n";
  return EXIT_SUCCESS;
}
