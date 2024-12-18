#include <stddef.h>
#include <stdint.h>

#include "polkavm_guest.h"

// Missing builtins

#define EVM_WORD_SIZE 32
#define ALIGN(size) ((size + EVM_WORD_SIZE - 1) & ~(EVM_WORD_SIZE - 1))
#define MAX_MEMORY_SIZE (64 * 1024)
static char __memory[MAX_MEMORY_SIZE];
static uint32_t __memory_size = 0;

void *  __sbrk_internal(uint32_t offset, uint32_t size) {
    if (offset >= MAX_MEMORY_SIZE || size > MAX_MEMORY_SIZE) {
        return NULL;
    }

    uint32_t new_size = ALIGN(offset + size);
    if (new_size > MAX_MEMORY_SIZE) {
        return NULL;
    }
    if (new_size > __memory_size) {
        __memory_size = new_size;
    }

    return (void *)&__memory[__memory_size];
}

uint32_t __msize() {
    return __memory_size;
}

void * memset(void *b, int c, size_t len) {
    uint8_t *dest = b;
    while (len-- > 0) *dest++ = c;
    return b;
}

void * memcpy(void *dst, const void *_src, size_t len) {
    uint8_t *dest = dst;
    const uint8_t *src = _src;

    while (len--) *dest++ = *src++;

    return dst;
}

void * memmove(void *dst, const void *src, size_t n) {
	char *d = dst;
	const char *s = src;

	if (d==s) return d;
	if ((uintptr_t)s-(uintptr_t)d-n <= -2*n) return memcpy(d, s, n);

	if (d<s) {
		for (; n; n--) *d++ = *s++;
	} else {
		while (n) n--, d[n] = s[n];
	}

	return dst;
}

// Imports

POLKAVM_IMPORT(void, address, uint32_t)

POLKAVM_IMPORT(void, balance, uint32_t)

POLKAVM_IMPORT(void, balance_of, uint32_t, uint32_t)

POLKAVM_IMPORT(void, block_hash, uint32_t, uint32_t)

POLKAVM_IMPORT(void, block_number, uint32_t)

POLKAVM_IMPORT(uint64_t, call, uint32_t)

POLKAVM_IMPORT(uint64_t, call_data_copy, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint64_t, call_data_load, uint32_t, uint32_t)

POLKAVM_IMPORT(uint64_t, call_data_size, uint32_t)

POLKAVM_IMPORT(uint64_t, delegate_call, uint32_t)

POLKAVM_IMPORT(void, caller, uint32_t)

POLKAVM_IMPORT(void, chain_id, uint32_t)

POLKAVM_IMPORT(void, code_size, uint32_t, uint32_t)

POLKAVM_IMPORT(void, code_hash, uint32_t, uint32_t)

POLKAVM_IMPORT(void, deposit_event, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, get_immutable_data, uint32_t, uint32_t);

POLKAVM_IMPORT(uint64_t, get_storage, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, hash_keccak_256, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint64_t, instantiate, uint32_t)

POLKAVM_IMPORT(void, now, uint32_t)

POLKAVM_IMPORT(void, origin, uint32_t)

POLKAVM_IMPORT(void, seal_return, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint64_t, ref_time_left)

POLKAVM_IMPORT(void, return_data_copy, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, return_data_size, uint32_t)

POLKAVM_IMPORT(void, set_immutable_data, uint32_t, uint32_t);

POLKAVM_IMPORT(uint64_t, set_storage, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, value_transferred, uint32_t)

POLKAVM_IMPORT(void, weight_to_fee, uint64_t, uint64_t, uint32_t);
