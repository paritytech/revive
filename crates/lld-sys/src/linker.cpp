#include "lld/Common/Driver.h"
#include "lld/Common/CommonLinkerContext.h"
#include "llvm/Support/CrashRecoveryContext.h"

LLD_HAS_DRIVER(elf);

extern "C" bool LLDELFLink(const char *argv[], size_t length)
{
    bool canRunAgain;

    {
        llvm::ArrayRef<const char *> args(argv, length);
        llvm::CrashRecoveryContext crc;
        if (!crc.RunSafely([&]()
                           { canRunAgain = lld::elf::link(args, llvm::outs(), llvm::errs(), false, false); }))
            return false;
    }

    llvm::CrashRecoveryContext crc;
    return canRunAgain && crc.RunSafely([&]()
                                        { lld::CommonLinkerContext::destroy(); });
}
