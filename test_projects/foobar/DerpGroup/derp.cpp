//
//  derp.cpp
//  derp
//
//  Created by Travers on 19/10/2024.
//

#include "derp.hpp"

#include "lol_print.hpp"
#include "shock.hpp"

#include <iostream>

int derp(int a, int b) {
    std::cout << "derp " << a << ' '<< b << '\n';
    lolprint();
    return a + shock(a, b);
}
