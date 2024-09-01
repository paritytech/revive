#include <stdbool.h>

#include "polkavm_guest.h"

extern void __entry(bool);

static void deploy() {
    __entry(true);
}

static void call() {
    __entry(false);
}

POLKAVM_EXPORT(void, deploy)
POLKAVM_EXPORT(void, call)
