#include <array>
#include <cstdlib>
#include <iostream>

#include "zstd.h"

#include "blobject.hpp"
#include "mylib.hpp"
#include "nasmproj.h"

int main(int argc, char**) {  //
  std::cout << MY_DEFINE << "\n";

  const auto ret = add_two(argc);
  std::cout << "add_two(argc) = " << ret << "\n\n";

  std::array<char, 100> fbuf{1, 2, 3, 4, 5, 6, 7, 8, 9};
  std::array<char, 100> cbuf{0};

  size_t const csz = ZSTD_compress(cbuf.data(), cbuf.size(), fbuf.data(), fbuf.size(), 1);

  std::cout << "ZSTD_compress size: " << csz << "\n";

  if (ZSTD_isError(csz)) {
    std::cout << "ZSTD error: " << ZSTD_getErrorName(csz) << "\n";
    return EXIT_FAILURE;
  }

  for (const auto c : fbuf) {
    std::cout << static_cast<int>(c) << " ";
  }
  std::cout << "\n\n";
  for (size_t i = 0; i < csz; i++) {
    std::cout << static_cast<int>(cbuf[i]) << " ";
  }
  std::cout << "\n\n";

  blobject::DoBlob1();
  blobject::DoBlob2();

  const int asm_result = asm_add_two_numbers(argc, argc);
  std::cout << "      argc: " << argc << "\n";
  std::cout << "asm_result: " << asm_result << "\n";

  return asm_result - (argc + argc);
}
