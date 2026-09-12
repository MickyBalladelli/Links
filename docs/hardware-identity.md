# Native hardware-backed identity custody

The native adapters are wired to the shared Rust identity store through
`links-identity-ffi`. The code is implemented; signed physical-device acceptance
remains open. Do not treat host fixture tests or cross-compilation as hardware
security evidence.

## Security boundary

The identity remains Ed25519. Apple Secure Enclave supplies a non-exportable P-256
wrapping key; AndroidKeyStore supplies a non-exportable AES-256-GCM wrapping key
whose KeyInfo must report TEE/StrongBox backing. There is no software-key fallback.
The sealed seed stays in device-only, non-synchronizing Keychain storage on Apple,
or AtomicFile records in Android's no-backup directory. Existing LKS1 records and
UUID handles are unchanged; there is no identity rotation or plaintext migration.

Only the wrapping key remains inside hardware. Ed25519 seeds are unwrapped into
app memory for each operation. The shared Rust implementation generates seeds
with the OS CSPRNG, signs with ed25519-dalek, and zeroizes its owned seed buffers.
Swift and JNI adapters wipe temporary seed copies on both success and error;
JNI preserves pending Java exceptions during wiping and failed-create cleanup.
Managed runtimes, Swift Data copy-on-write, OS buffers and snapshots mean this is
not a guarantee that every historical copy is erased. An attacker controlling the
running unlocked app can still request signatures or obtain unwrapped seeds.

No seed is cached between calls. The application-facing API exposes only identity
creation, reference validation, checked signing and deletion. Creation reads back
the newly sealed seed before returning; failed readback attempts cleanup. Signing
checks the expected enrolled public key against the very same unwrapped seed it
uses to sign, avoiding a separate check/sign race. Missing, locked, tampered,
software-backed, invalidated or substituted identities fail closed. Restoration
never calls creation. Domain separation and account binding are the caller's
responsibility; use the existing phone-auth and enrollment transcript contracts.

Android additionally requires a configured secure screen lock and an unlocked
user profile. It retains `setUnlockedDeviceRequired(true)` on every supported
version rather than weakening storage policy on older devices. Android 12–14
have documented availability issues with this flag: removing the screen lock
can delete keys, and weak-biometric unlock may not authorize use. A retry after
PIN/password unlock is appropriate for a locked key; permanent loss requires
authenticated pairing/recovery, never regeneration under the old identity.
The explicit Keyguard check supplements, not replaces, Keystore enforcement.

## Integration

The dependency flow is:

`Swift / Java HardwareIdentityStore -> C ABI / JNI -> Rust HardwareIdentityStore -> native vault callbacks`

`crates/identity-ffi` owns the synchronous C ABI. Its header is
`native/apple/Sources/CLinksIdentity/links_identity.h`, also used by Android CMake.
Callbacks execute on the entering thread, cannot retain buffers or throw across
C, and are never retained by Rust. The ABI checks null pointers, callback version,
canonical handle bytes and the 1 MiB identity-transcript limit. Like any C API, it
requires valid, correctly sized, nonoverlapping buffers from trusted native code.

Both native wrappers serialize operations. Run them on a background identity
worker, not the UI thread. Keep all identity operations in one app process; the
wrapper locks are not a cross-process transaction mechanism. The lower-level
seed-vault APIs are infrastructure APIs, not an application seed-export feature.

Swift callers use `createIdentity()`, `validateIdentity(_:)`,
`sign(_:transcript:)` and `deleteIdentity(_:)`. Java callers use the same names
with `HardwareIdentityStore.KeyReference`. Persist the handle and public key with
the authenticated account/device/node binding before sending enrollment requests.
Only those public references belong in application metadata. Reconstruct the
reference and validate it after restart. Never call creation on ordinary login,
network retry, missing metadata or unwrap failure. No mobile onboarding UI exists
yet; that remains in the Android/iOS client phases.

Deletion removes the wrapping key and sealed record; repeated deletion is safe.
Only call it for explicit device removal/reset. Keep the reference if deletion
reports failure so cleanup can be retried. Process death between native key
creation and metadata persistence can leave orphan keys; automatic orphan
reconciliation and account recovery are not implemented here. Uninstall, restored
backups or hardware loss must not silently create a replacement identity.

## Builds

The Apple Swift package requires the matching Rust static library to be built
first. Its default library search path is the workspace's `target/debug` for host
tests. Set `LINKS_IDENTITY_LIB_DIR` to an absolute target/profile directory for an
Xcode/iOS build. Do not mix the macOS arm64 archive with an iOS arm64 archive.
The local source package uses a linker search flag; binary/XCFramework distribution
packaging is a separate client-release task.

```sh
cargo build -p links-identity-ffi --locked
swift test --package-path native/apple
rustup target add aarch64-apple-ios
cargo build -p links-identity-ffi --target aarch64-apple-ios --release --locked
# For the signed iOS host, set LINKS_IDENTITY_LIB_DIR to the absolute directory:
# target/aarch64-apple-ios/release
```

Android supports arm64-v8a and x86_64. Gradle invokes CMake, which builds the Rust
static archive for each ABI and links the JNI shared library into the AAR. No
prebuilt secret-bearing library or local machine path is committed. JNI names
and callback methods are preserved through consumer R8 rules.

```sh
rustup target add aarch64-linux-android x86_64-linux-android
sdkmanager "platforms;android-35" "build-tools;35.0.0" "ndk;28.0.13004108" "cmake;3.22.1"
# JDK 17, Gradle 8.11.1; cargo must be on PATH.
gradle -p native/android assembleDebug assembleDebugAndroidTest
# Unlocked physical device with secure screen lock and TEE/StrongBox:
gradle -p native/android connectedDebugAndroidTest
```

## Automated verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo check -p links-client-core --target wasm32-unknown-unknown --locked
bash native/android/tests/run-host-tests.sh
```

Rust FFI tests cover lifecycle, callback failure, create-readback cleanup, malformed
inputs, output clearing, checked signing, deletion failures and panic containment.
Swift tests exercise the real Rust archive with an internal fixture vault and
verify signatures independently with CryptoKit. The host JNI test runs the real
C/JNI/Rust path under JVM `-Xcheck:jni`, verifies Ed25519 signatures independently,
and checks managed-array wiping, exception preservation, deletion and failure
cleanup. Test fixtures are not exposed by production constructors.

Local verification for this change: 32 Rust unit tests passed; 14 database tests
were skipped (unrelated PostgreSQL integration). Formatting, warning-free Clippy,
doc tests and the shared-core WASM check passed. Six Apple tests passed; three
hardware tests were skipped. The Swift wrapper linked successfully for arm64 iOS,
and Rust archives cross-built for arm64 iOS, arm64 Android and x86_64 Android.
JNI host tests and Java production compilation against Android API classes passed.
The complete Gradle/NDK build and Android instrumentation tests were not run
locally: Gradle, Android SDK/NDK and an Android device were not available. CI now
includes those build steps, but hosted CI has not been run in this session.

## Physical acceptance gate — still required

Run the opt-in Apple tests in an entitled, signed physical iOS test host with
`LINKS_TEST_SECURE_ENCLAVE=1`; unsigned macOS SwiftPM tests are not a substitute.
Android instrumentation tests intentionally fail on software-only Keystore
rather than skipping that security requirement. Tests cover hardware round-trip,
wrapper recreation, signing, tamper rejection, key loss and deletion. Wrapper
recreation is not a process restart; the following manual device checks remain:

1. Persist a newly created reference, force-stop/terminate the app, relaunch and
   confirm the same public key and valid signatures without creating another key.
2. Lock the device while the test process can run and confirm unwrap/signing fails.
   Unlock with the device credential and confirm the existing identity works.
   Repeat after reboot, before and after the first unlock; background suspension
   alone is not evidence of a rejected cryptographic operation.
3. Exercise device lock removal/key invalidation on a disposable test identity,
   record the failure and confirm no new key appears under the existing reference.
4. Copy sealed records and metadata to a second device/backup restoration and
   confirm they cannot recover the original signing identity. Do not log seeds.
5. Record OS version, device model, signing/entitlement configuration, hardware
   security level and pass/fail outcomes. Verify minified Android packaging too.

Keep the master hardware TODO open until this evidence is recorded. No public
release should bypass the Phase 1 security review or eventual cryptographic audit.

## Platform references

- [Apple Secure Enclave key restrictions](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave)
- [Android Keystore hardware security](https://developer.android.com/privacy-and-security/keystore)
- [Unlocked-device-required behavior and older-version issues](https://developer.android.com/reference/android/security/keystore/KeyGenParameterSpec.Builder#setUnlockedDeviceRequired(boolean))
