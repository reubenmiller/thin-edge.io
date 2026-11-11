#!/bin/sh
# Build compiler-rt's lock-based atomics (__atomic_*_8) as a static library.
# 32-bit MIPS has no 64-bit atomic instructions, and QuickJS's Atomics use them.
# Usage: build-libatomic.sh <clang target> <output dir> [extra cflags...]
set -eu
TARGET="$1"; OUT="$2"; shift 2
LLVM_REF="${LLVM_REF:-llvmorg-18.1.8}"
WORK="$(mktemp -d)"
cd "$WORK"
for f in atomic.c assembly.h; do
    curl -fsSL -o "$f" "https://raw.githubusercontent.com/llvm/llvm-project/$LLVM_REF/compiler-rt/lib/builtins/$f"
done
mkdir -p "$OUT"
clang --target="$TARGET" "$@" -O2 -fPIC -c atomic.c -o atomic.o
llvm-ar rcs "$OUT/libatomic.a" atomic.o
echo "built $OUT/libatomic.a"
