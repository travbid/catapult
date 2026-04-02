#include "add.hpp"
#include "divide.hpp"
#include "multiply.hpp"
#include "subtract.hpp"

#include <iostream>

int main(int argc, char **) {
  std::cout << "Hello, world\n";

  const auto added = add(argc, argc);
  const auto multiplied = multiply(added, added);
  const auto divided = divide(multiplied, added);
  const auto subtracted = subtract(divided, added);
  return subtracted;
}
