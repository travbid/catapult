//
//  shock.cpp
//  shock
//
//  Created by Travers on 19/10/2024.
//

#include "shock.hpp"

#include "wing.hpp"

#include <iostream>

int shock(int a, int b) {
    std::cout << "shock " << a << ' '<< b << '\n';
    return 5 + wing(a, b);
}
