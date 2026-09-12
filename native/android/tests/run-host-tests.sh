#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
JAVA_HOME="${JAVA_HOME:-$(java -XshowSettings:properties -version 2>&1 | sed -n 's/^[[:space:]]*java.home = //p')}"
: "${JAVA_HOME:?Set JAVA_HOME to a JDK with JNI headers}"
OUTPUT="$ROOT/target/jni-host-tests"
mkdir -p "$OUTPUT/classes"
cargo build --manifest-path "$ROOT/Cargo.toml" -p links-identity-ffi --target-dir "$ROOT/target" --locked
case "$(uname -s)" in
    Darwin) JNI_PLATFORM=darwin; LIBRARY=liblinks_identity_jni.dylib; EXTRA=(-framework Security -framework CoreFoundation) ;;
    Linux) JNI_PLATFORM=linux; LIBRARY=liblinks_identity_jni.so; EXTRA=(-ldl -lm -lpthread) ;;
    *) echo "Host JNI tests support macOS and Linux" >&2; exit 1 ;;
esac
cc -std=c11 -Wall -Wextra -Werror -shared -fPIC \
    -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/$JNI_PLATFORM" \
    -I"$ROOT/native/apple/Sources/CLinksIdentity" \
    "$ROOT/native/android/src/main/cpp/identity_jni.c" \
    "$ROOT/target/debug/liblinks_identity_ffi.a" "${EXTRA[@]}" -o "$OUTPUT/$LIBRARY"
"$JAVA_HOME/bin/javac" --release 17 -Xlint:all -Werror -d "$OUTPUT/classes" \
    "$ROOT/native/android/src/main/java/ai/links/identity/NativeIdentityBridge.java" \
    "$ROOT/native/android/tests/BridgeSmokeTest.java"
"$JAVA_HOME/bin/java" --enable-native-access=ALL-UNNAMED -Xcheck:jni -Djava.library.path="$OUTPUT" -cp "$OUTPUT/classes" ai.links.identity.BridgeSmokeTest
