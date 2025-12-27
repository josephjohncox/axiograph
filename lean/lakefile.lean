import Lake
open Lake DSL

package axiograph where
  moreServerArgs := #["-Kserver=120000"]

require mathlib from git
  "https://github.com/leanprover-community/mathlib4" @ "v4.26.0"

@[default_target]
lean_lib Axiograph where
  roots := #[`Axiograph]

lean_exe axiograph_verify where
  root := `Axiograph.VerifyMain

/--
Build the trusted checker executable `axiograph_verify`.

On macOS, Lean's bundled `clang` needs a valid macOS SDK to compile/link the
generated C output. When `SDKROOT` is not set, you may see errors like:

```
fatal error: 'stdio.h' file not found
```

This script sets `SDKROOT` via `xcrun` (if on macOS) and then runs:

```
lake build axiograph_verify
```

Recommended alternative (repo root): `make lean-exe` (also handles `SDKROOT`).
-/
script buildExe (_args) do
  let uname := (← IO.Process.output { cmd := "uname", args := #[] }).stdout.trim
  let mut extraEnv : Array (String × Option String) := #[]
  if uname == "Darwin" then
    let sdkOut :=
      (← IO.Process.output { cmd := "xcrun", args := #["--sdk", "macosx", "--show-sdk-path"] })
    let sdk := sdkOut.stdout.trim
    if sdk.isEmpty then
      IO.eprintln "error: failed to locate macOS SDK via `xcrun --sdk macosx --show-sdk-path`."
      IO.eprintln "hint: install Xcode Command Line Tools: `xcode-select --install`."
      return 2
    extraEnv := extraEnv.push ("SDKROOT", some sdk)
    if (← IO.getEnv "MACOSX_DEPLOYMENT_TARGET").isNone then
      extraEnv := extraEnv.push ("MACOSX_DEPLOYMENT_TARGET", some "13.0")

  let child ←
    IO.Process.spawn
      { cmd := "lake"
        args := #["build", "axiograph_verify"]
        env := extraEnv
        stdin := .inherit
        stdout := .inherit
        stderr := .inherit }
  return (← child.wait)
