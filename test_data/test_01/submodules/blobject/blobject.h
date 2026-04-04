#pragma once

#if defined(__cplusplus)

namespace blobject {

void DoBlob1();

}

extern "C" {
#endif

void blobject_DoBlob2();

#if defined(__cplusplus)
}
#endif
