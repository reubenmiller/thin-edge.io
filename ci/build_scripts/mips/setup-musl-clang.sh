#!/bin/sh
# Build a 32-bit MIPS musl sysroot with clang, plus libunwind.a and libatomic.a,
# and a clang+lld linker wrapper for rustc.
# Usage: setup-musl-clang.sh <mips|mipsel> <prefix> <libunwind source dir (from rust-src)>
set -eu
ARCH="$1"; PREFIX="$2"; LIBUNWIND_SRC="$3"
MUSL_VERSION="${MUSL_VERSION:-1.2.5}"
TRIPLE="$ARCH-linux-musl"
# Must match rustc's {mips,mipsel}-unknown-linux-musl targets: "+mips32r2,+soft-float"
ARCH_FLAGS="-march=mips32r2 -msoft-float"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"

# musl
cd "$WORK"
curl -fsSL "https://musl.libc.org/releases/musl-${MUSL_VERSION}.tar.gz" | tar xz
cd "musl-${MUSL_VERSION}"
CC="clang --target=$TRIPLE $ARCH_FLAGS" AR=llvm-ar RANLIB=llvm-ranlib \
    ./configure --target="$TRIPLE" --prefix="$PREFIX" --disable-shared >/dev/null
make -j"$(nproc)" >/dev/null
make install >/dev/null

# libunwind.a: std's unwind crate links it statically for musl targets,
# but rust doesn't ship it for tier 3 targets. Flags follow rust's bootstrap.
mkdir -p "$WORK/unwind" && cd "$WORK/unwind"
UNWIND_FLAGS="--target=$TRIPLE $ARCH_FLAGS --sysroot=$PREFIX -O2 -fPIC -I$LIBUNWIND_SRC/include \
    -funwind-tables -fvisibility=hidden -fstrict-aliasing \
    -D_LIBUNWIND_DISABLE_VISIBILITY_ANNOTATIONS -D_LIBUNWIND_IS_NATIVE_ONLY -DNDEBUG"
for f in "$LIBUNWIND_SRC"/src/*.c; do
    case "$f" in *Sgx*) continue ;; esac
    # shellcheck disable=SC2086
    clang $UNWIND_FLAGS -std=c99 -c "$f" -o "$(basename "$f").o"
done
for f in "$LIBUNWIND_SRC"/src/*.cpp; do
    # shellcheck disable=SC2086
    clang $UNWIND_FLAGS -std=c++17 -nostdinc++ -fno-exceptions -fno-rtti -c "$f" -o "$(basename "$f").o"
done
for f in "$LIBUNWIND_SRC"/src/*.S; do
    # shellcheck disable=SC2086
    clang $UNWIND_FLAGS -c "$f" -o "$(basename "$f").o"
done
llvm-ar rcs "$PREFIX/lib/libunwind.a" ./*.o

# libatomic.a
# shellcheck disable=SC2086
sh "$SCRIPT_DIR/build-libatomic.sh" "$TRIPLE" "$PREFIX/lib" $ARCH_FLAGS --sysroot="$PREFIX"

# Linker wrapper. Its name must not end in "-ld", or rustc treats it as a raw ld.
# rustc runs with -C link-self-contained=no, so the start files come from the
# musl sysroot; crtn.o must come last.
mkdir -p "$PREFIX/bin"
cat > "$PREFIX/bin/$TRIPLE-clang" <<WRAPPER
#!/bin/sh
exec clang --target=$TRIPLE $ARCH_FLAGS --sysroot=$PREFIX -fuse-ld=lld -static -nostartfiles \\
    $PREFIX/lib/crt1.o $PREFIX/lib/crti.o "\$@" -latomic $PREFIX/lib/crtn.o
WRAPPER
chmod +x "$PREFIX/bin/$TRIPLE-clang"
echo "musl sysroot installed to $PREFIX"
