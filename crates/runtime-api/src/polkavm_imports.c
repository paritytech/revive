#include <stddef.h>
#include <stdint.h>

#include "polkavm_guest.h"


// Missing builtins

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

void *  __sbrk(uint32_t size) {
    uint32_t address;
    __asm__ __volatile__(
            ".insn r 0xb, 1, 0, %[dst], %[sz], zero"
            : [dst] "=r" (address)
            : [sz] "ir" (size)
            :
    );
    return (void *)address;
}


// Imports

POLKAVM_IMPORT(void, input, uint32_t, uint32_t)

POLKAVM_IMPORT(void, seal_return, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, return_data_copy, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, return_data_size, uint32_t)

POLKAVM_IMPORT(void, set_immutable_data, uint32_t, uint32_t);

POLKAVM_IMPORT(void, get_immutable_data, uint32_t, uint32_t);

POLKAVM_IMPORT(void, value_transferred, uint32_t)

POLKAVM_IMPORT(uint32_t, set_storage, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, get_storage, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, clear_storage, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, contains_storage, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, take_storage, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, call, uint32_t)

POLKAVM_IMPORT(uint32_t, delegate_call, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, instantiate, uint32_t)

POLKAVM_IMPORT(void, terminate, uint32_t)

POLKAVM_IMPORT(void, caller, uint32_t)

POLKAVM_IMPORT(uint32_t, is_contract, uint32_t)

POLKAVM_IMPORT(uint32_t, code_hash, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, code_size, uint32_t)

POLKAVM_IMPORT(void, own_code_hash, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, caller_is_origin)

POLKAVM_IMPORT(uint32_t, caller_is_root)

POLKAVM_IMPORT(void, address, uint32_t)

POLKAVM_IMPORT(void, weight_to_fee, uint64_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, gas_left, uint32_t, uint32_t)

POLKAVM_IMPORT(void, balance, uint32_t)

POLKAVM_IMPORT(void, chain_id, uint32_t)

POLKAVM_IMPORT(void, now, uint32_t)

POLKAVM_IMPORT(void, minimum_balance, uint32_t, uint32_t)

POLKAVM_IMPORT(void, deposit_event, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, block_number, uint32_t)

POLKAVM_IMPORT(void, hash_sha2_256, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, hash_keccak_256, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, hash_blake2_256, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(void, hash_blake2_128, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, call_chain_extension, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, debug_message, uint32_t, uint32_t)

POLKAVM_IMPORT(uint32_t, set_code_hash, uint32_t)

POLKAVM_IMPORT(uint64_t, instantiation_nonce,)

POLKAVM_IMPORT(uint32_t, transfer, uint32_t, uint32_t, uint32_t, uint32_t)
