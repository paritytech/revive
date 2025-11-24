# `revive-runner` sandbox

Running contract code usually requires a blockchain node. While local dev nodes can be used, sometimes it's just not desirable to do so. Instead, it can be much more convenient to run and debug contract code with a stripped down environment.

This is where the `revive-runner` comes in handy. In a nutshell, it is a single-binary no-blockchain `pallet-revive` runtime.

## Installation and usage

Inside the root `revive` repository directory, install it from source (requires Rust installed):

```bash
make install-revive-runner
```

After installing, see `revive-runner --help` for usage help.

## Trace logs

The standard `RUST_LOG` environment variable controls the log output from the contract execution. This includes `revive` runtime logs and PVM execution trace logs. Sometimes it's convenient to have more fine granular insight. Some useful filters:
- `RUST_LOG=runtime=trace`: The `pallet-revive` runtime trace logs.
- `RUST_LOG=polkavm=trace`: Low level PolkaVM instruction tracing.

## Automatic contract instantiation

To avoid running the constract in an unitialized state, `revive-runner` automatically instantiates the contract before calling it (constructor arguments can be provided).

## Example

Suppose we want to trace the syscalls of the execution of a compiled contract file `Flipper.pvm`:

```bash
RUST_LOG=runtime=trace revive-runner -f Flipper.pvm
[DEBUG runtime::revive] Contract memory usage: purgable=6144/3145728 KB baseline=103063/1572864
[TRACE runtime::revive::strace] call_data_size() = Ok(0) gas_consumed: Weight { ref_time: 985209, proof_size: 0 }
[TRACE runtime::revive::strace] value_transferred(out_ptr: 4294836096) = Ok(()) gas_consumed: Weight { ref_time: 2937634, proof_size: 0 }
[TRACE runtime::revive::strace] call_data_copy(out_ptr: 131216, out_len: 0, offset: 0) = Ok(()) gas_consumed: Weight { ref_time: 4084483, proof_size: 0 }
[TRACE runtime::revive::strace] seal_return(flags: 0, data_ptr: 131216, data_len: 0) = Err(TrapReason::Return(ReturnData { flags: 0, data: [] })) gas_consumed: Weight { ref_time: 5510615, proof_size: 0 }
[TRACE runtime::revive] frame finished with: Ok(ExecReturnValue { flags: (empty), data: [] })
[TRACE runtime::revive::strace] call_data_size() = Ok(0) gas_consumed: Weight { ref_time: 985209, proof_size: 0 }
[TRACE runtime::revive::strace] seal_return(flags: 1, data_ptr: 131088, data_len: 0) = Err(TrapReason::Return(ReturnData { flags: 1, data: [] })) gas_consumed: Weight { ref_time: 2456669, proof_size: 0 }
[TRACE runtime::revive] frame finished with: Ok(ExecReturnValue { flags: REVERT, data: [] })
```



