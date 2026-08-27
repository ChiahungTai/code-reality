# Upstream issue draft — sourcegraph/scip-python

Title: `RangeError: Maximum call stack size exceeded` in
`assignClassToProtocol` on protocol-heavy corpus (v0.6.6)

## Environment

- scip-python 0.6.6 (npm `@sourcegraph/scip-python`)
- Node v24.10.0, macOS arm64
- Private Python repo (~550 files, heavy `typing.Protocol` /
  `typing.Generic` usage, polars/pandas-heavy)

## Reproduction

Indexing aborts with a fatal error:

```
An internal error occurred while type checking file
"<repo>/mosaic_alpha/model/utils.py":
RangeError: Maximum call stack size exceeded
    at overload (.../typeEvaluator.ts:23401:51)
    at Array.forEach (<anonymous>)
    at callback (.../typeEvaluator.ts:23382:27)
    at s.timeOperation (.../timing.ts:40:20)
    at <anonymous> (.../protocols.ts:384:44)
    at Map.forEach (<anonymous>)
    at mroClass (.../protocols.ts:186:33)
    at Array.forEach (<anonymous>)
    at assignClassToProtocolInternal (.../protocols.ts:175:26)
    at assignClassToProtocol (.../protocols.ts:94:24)
```

The analysis phase tolerates the error ("Analysis partially failed"), but
the emit phase re-throws it as a fatal error and the whole index is lost.

## Analysis

The recursion cycle is
`assignClassToProtocol → assignClassToProtocolInternal → mroClass
(member comparison) → assignType → overload evaluation →
assignClassToProtocol`. Neither existing guard stops it:

- the `recursionCount` cap is threaded through `assignType` but reset by
  overload evaluation, so it never accumulates across the cycle;
- the `protocolAssignmentStack` same-`(srcType, destType)` check never
  repeats because generic specialization keeps producing fresh types.

`--stack-size` is not a workaround: `NODE_OPTIONS` rejects it, and a
direct `node --stack-size=...` invocation segfaults (exit 139) before
any `RangeError`.

Additionally, per-file internal errors during the emit phase abort the
entire index (`throw { currentFilepath, error }` in `indexer.ts`); we
suggest recording and skipping the file instead, with a loud summary of
skipped files so consumers can detect partial indexes.

A depth guard patch (module-level `protocolAssignDepth` in protocols.ts)
makes the same corpus index to completion in ~25s with 11/552 files
skipped (reported loudly). Happy to send a PR if this approach sounds
acceptable.
