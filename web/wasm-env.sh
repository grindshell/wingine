# Sets the environment for building Luau, through mlua, into a wasm32-unknown-unknown module, with
# WASI SDK's C and C++ compilers and its WASI Preview 1 libc, libc++ and Wasm exceptions, as mlua's
# tests/wasm/setup.sh does. Source it: `. web/wasm-env.sh`. `build.sh` does.
#
# WASI SDK, 34 or later, is found through WASI_SDK_PATH, or else as the last wasi-sdk-* folder in
# ~/.local/wasi-sdk. On Windows, run it from Git Bash.

if [ -z "${WASI_SDK_PATH:-}" ]; then
  WASI_SDK_PATH=$(ls -d "$HOME"/.local/wasi-sdk/wasi-sdk-* 2>/dev/null | sort | tail -n 1)
  if [ -z "$WASI_SDK_PATH" ]; then
    echo "Set WASI_SDK_PATH to a WASI SDK (https://github.com/WebAssembly/wasi-sdk/releases), version 34 or later" >&2
    return 1
  fi
fi
export WASI_SDK_PATH

wasi_sdk=$WASI_SDK_PATH
wasi_exe=
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*)
    # Cargo and the compilers it runs are Windows programs, which take C:/ paths.
    wasi_sdk=$(cygpath -m "$wasi_sdk")
    wasi_exe=.exe
    ;;
esac
wasi_target=wasm32-wasip1
wasi_lib=$wasi_sdk/share/wasi-sysroot/lib/$wasi_target

export CC_wasm32_unknown_unknown=$wasi_sdk/bin/clang$wasi_exe
export CXX_wasm32_unknown_unknown=$wasi_sdk/bin/clang++$wasi_exe
export AR_wasm32_unknown_unknown=$wasi_sdk/bin/llvm-ar$wasi_exe
export CXXSTDLIB_wasm32_unknown_unknown=c++
export CFLAGS_wasm32_unknown_unknown=--target=$wasi_target
# The SDK's standard Wasm exception ABI, which Luau's errors need:
# https://github.com/WebAssembly/wasi-sdk/blob/wasi-sdk-34/CppExceptions.md
export LUAU_CXXFLAGS="--target=$wasi_target -fexceptions -fwasm-exceptions -mllvm -wasm-use-legacy-eh=false"
# Rust's flags: the libraries the C++ code links against, and web-sys's unstable APIs, such as
# WebGPU's, which wgpu uses. They're split at spaces, so the SDK's path can't have any.
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-Lnative=$wasi_lib/eh -Lnative=$wasi_lib \
-lstatic=c++abi -lstatic=unwind -lstatic=wasi-emulated-process-clocks -lstatic=c \
--cfg=web_sys_unstable_apis"
unset wasi_sdk wasi_exe wasi_target wasi_lib
