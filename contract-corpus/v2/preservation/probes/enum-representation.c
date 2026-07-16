#include "../input/preservation.h"

_Static_assert(
    __builtin_types_compatible_p(__typeof__((enum parc_mode)0), unsigned int),
    "GCC target must represent parc_mode as unsigned int"
);
_Static_assert(sizeof(enum parc_mode) == 4, "parc_mode storage must be 32 bits");
_Static_assert(_Alignof(enum parc_mode) == 4, "parc_mode alignment must be 32 bits");

int parc_preservation_enum_probe(void) {
    return PARC_MODE_FAST;
}
