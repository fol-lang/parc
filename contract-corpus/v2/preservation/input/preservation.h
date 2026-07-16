#ifndef FOLLANG_PARC_PRESERVATION_H
#define FOLLANG_PARC_PRESERVATION_H

#define PARC_ABI_LEVEL 7

struct parc_opaque;
typedef const volatile struct parc_opaque *parc_handle;

struct parc_packet {
    int value;
};

enum parc_mode {
    PARC_MODE_FAST = 7
};

parc_handle parc_open(parc_handle restrict handle)
    __attribute__((nonnull(1), ms_abi));
struct parc_opaque *parc_missing(struct parc_opaque *handle);

#if defined(PRESERVATION_PARTIAL)
__float128 __attribute__((preserve_most)) parc_extended(__int128 wide);
#endif

#endif
