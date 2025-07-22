# revive-explorer

The `revive-explorer` is a helper utility for exploring the compilers YUL lowering unit.

It analyzes a given shared objects from the debug dump and outputs:
- The count of each YUL statement translated.
- A per YUL statement break-down of bytecode size contributed per.
- Estimated `yul-phaser` cost parameters.

Example:

```
statements count:
	block 532
	Caller 20
	Not 73
	Gas 24
	Shr 2
    ...
	Shl 259
	SetImmutable 2
	CodeSize 1
	CallDataLoad 87
	Return 56
bytes per statement:
	Or 756
	CodeCopy 158
	Log3 620
	Return 1562
	MStore 36128
	...
	ReturnDataCopy 2854
	DataOffset 28
	assignment 1194
	Number 540
	CallValue 4258
yul-phaser parameters:
	--break-cost 1
	--variable-declaration-cost 3
	--function-call-cost 8
	--if-cost 4
	--expression-statement-cost 6
	--function-definition-cost 11
	--switch-cost 3
	--block-cost 1
	--leave-cost 1
	--assignment-cost 1
```

