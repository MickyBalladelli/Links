cmake_minimum_required(VERSION 3.22.1)
project(links_identity_jni C)
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
get_filename_component(LINKS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." ABSOLUTE)
if(ANDROID_ABI STREQUAL "arm64-v8a")
    set(RUST_TARGET aarch64-linux-android)
elseif(ANDROID_ABI STREQUAL "x86_64")
    set(RUST_TARGET x86_64-linux-android)
else()
    message(FATAL_ERROR "Unsupported Links identity ABI: ${ANDROID_ABI}")
endif()
find_program(CARGO_EXECUTABLE cargo REQUIRED)
set(RUST_OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/rust")
set(RUST_LIBRARY "${RUST_OUTPUT}/${RUST_TARGET}/release/liblinks_identity_ffi.a")
string(TOUPPER "${RUST_TARGET}" RUST_TARGET_ENV)
string(REPLACE "-" "_" RUST_TARGET_ENV "${RUST_TARGET_ENV}")
file(GLOB_RECURSE RUST_INPUTS CONFIGURE_DEPENDS
    "${LINKS_ROOT}/crates/*.rs" "${LINKS_ROOT}/crates/*/Cargo.toml" "${LINKS_ROOT}/proto/*.proto")
add_custom_command(OUTPUT "${RUST_LIBRARY}"
    COMMAND "${CMAKE_COMMAND}" -E env
        "CARGO_TARGET_DIR=${RUST_OUTPUT}"
        "CARGO_TARGET_${RUST_TARGET_ENV}_LINKER=${CMAKE_C_COMPILER}"
        "${CARGO_EXECUTABLE}" build --manifest-path "${LINKS_ROOT}/Cargo.toml"
        -p links-identity-ffi --target "${RUST_TARGET}" --release --locked
    DEPENDS ${RUST_INPUTS} "${LINKS_ROOT}/Cargo.toml" "${LINKS_ROOT}/Cargo.lock"
    WORKING_DIRECTORY "${LINKS_ROOT}"
    VERBATIM)
add_custom_target(links_rust_build DEPENDS "${RUST_LIBRARY}")
add_library(links_rust STATIC IMPORTED)
set_target_properties(links_rust PROPERTIES IMPORTED_LOCATION "${RUST_LIBRARY}")
add_dependencies(links_rust links_rust_build)
add_library(links_identity_jni SHARED identity_jni.c)
target_include_directories(links_identity_jni PRIVATE "${LINKS_ROOT}/native/apple/Sources/CLinksIdentity")
target_compile_options(links_identity_jni PRIVATE -Wall -Wextra -Werror)
# Rust --print native-static-libs for the Android targets, including panic unwind.
target_link_libraries(links_identity_jni PRIVATE links_rust c m dl log unwind dl m c)
