# Native build prerequisites

The build is macOS-native. Minimum app runtime is macOS 14.4 because the reused
Core Audio process-tap API requires that version. Apple Silicon is the locally
verified development architecture; a separate Intel build has not been verified.

Observed on the development machine on October 16, 2026:

| Component | Observed configuration |
| --- | --- |
| macOS | 26.5.2 (25F84) |
| Xcode developer directory | `/Applications/Xcode.app/Contents/Developer` |
| macOS SDK | 26.5 |
| Rust | Project pins 1.97.0 in `rust-toolchain.toml` |
| Node.js | 22.18.0 |
| pnpm | 10.29.3 |
| CMake | 4.2.1 |

Install Xcode/Command Line Tools and complete their license/first-run setup.
A working Clang/C++ toolchain and CMake are needed for whisper.cpp; Metal is
compiled for the Whisper recognizer. Tauri uses the system WebKit framework.
The build script links AVFoundation and CoreAudio; macOS Keychain integration
uses Security.framework through the Rust security-framework dependency.

Use the project directory or `cargo +1.97.0 ...` explicitly: the shell's global
Rust compiler was 1.82.0, which does not represent the pinned project toolchain.
Use `pnpm install --frozen-lockfile` for the Tauri CLI dependency, then the
project's `pnpm dev`, `pnpm build`, or `pnpm test` commands. Root installation and
packaging instructions describe the tested artifact and exact install command.

The first dependency build needs network access to crates.io, pinned Git
sources (including cidre), and ONNX Runtime's native build dependency artifacts.
These are build dependencies, not speech model downloads. `ort` links its native
runtime at build time; normal app startup does not fetch ONNX Runtime or model
weights. Cargo.lock and pnpm-lock.yaml should be retained for reproducibility.
No FFmpeg executable, Python runtime, Meetily installation, or upstream meeting
backend is required by the app.

Ollama is a separate runtime service for local coaching. Live Coach does not
install Ollama or pull an Ollama model automatically. Speech recognition needs a
compatible existing Parakeet or Whisper model, selected through model discovery.
The model file candidate check precedes actual decoder loading; unsupported or
corrupt files produce a loading error before audio capture opens.

Developer ID signing and Apple notarization require valid local credentials and
Apple service access. A signing identity alone is not proof of notarization.
Verify the final bundle with `codesign --verify --deep --strict`, inspect its
signing identity, and separately verify Gatekeeper/notarization status before
reporting distribution status. See the root README for this build's actual
signing/notarization result, rather than inferring it from configuration.
