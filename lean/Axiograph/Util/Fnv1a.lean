import Std

/-!
# `Axiograph.Util.Fnv1a`

Certificates are snapshot-scoped: we need a stable way to bind a certificate to
the exact `.axi` input it was computed from.

For the initial migration we use a **simple, deterministic, non-cryptographic**
digest:

* algorithm: **FNV-1a 64-bit**
* input: the UTF-8 bytes of the `.axi` file as read
* output: `"fnv1a64:<16 lowercase hex digits>"`

This choice is pragmatic:

* easy to implement in both Rust and Lean,
* fully deterministic,
* sufficient for stable snapshot identity in the verification pipeline.

It is *not* a security primitive; we can upgrade to a cryptographic hash once we
have a vetted Lean implementation/library dependency in-tree.
-/

namespace Axiograph.Util

namespace Fnv1a

def digestPrefix : String := "fnv1a64:"
def factIdPrefix : String := "factfnv1a64:"

def offsetBasis : UInt64 := 0xcbf29ce484222325
def prime : UInt64 := 0x00000100000001b3

def hashBytes (bytes : ByteArray) : UInt64 :=
  Id.run do
    let mut h : UInt64 := offsetBasis
    for b in bytes do
      h := (h ^^^ (UInt64.ofNat b.toNat)) * prime
    return h

def hexDigit (n : Nat) : Char :=
  if n < 10 then
    Char.ofNat (n + ('0'.toNat))
  else
    Char.ofNat ((n - 10) + ('a'.toNat))

def toHexFixed16 (h : UInt64) : String :=
  -- Emit exactly 16 hex digits (big-endian) to match Rust formatting `{:016x}`.
  let shifts : List Nat := [60, 56, 52, 48, 44, 40, 36, 32, 28, 24, 20, 16, 12, 8, 4, 0]
  let chars :=
    shifts.map (fun shift =>
      let nibble : Nat := ((h >>> (UInt64.ofNat shift)) &&& 0x0f).toNat
      hexDigit nibble)
  String.ofList chars

def digestTextV1 (text : String) : String :=
  let h := hashBytes text.toUTF8
  digestPrefix ++ toHexFixed16 h

/-!
## Stable fact ids (`axi_fact_id_v1`)

For `.axi`-anchored certificates we also need a stable identifier for individual
*tuples/facts* inside an instance.

Rust computes this in `axiograph-dsl/src/digest.rs::axi_fact_id_v1`. We keep the
Lean implementation structurally aligned so certificates can be checked against
canonical `.axi` inputs without depending on PathDB-internal numeric IDs.
-/

def hashString (h : UInt64) (s : String) : UInt64 :=
  Id.run do
    let mut h := h
    for b in s.toUTF8 do
      h := (h ^^^ (UInt64.ofNat b.toNat)) * prime
    return h

def axiFactIdV1
    (moduleName schemaName instanceName relationName : String)
    (fieldsInDeclOrder : Array (String × String)) : String :=
  Id.run do
    let mut h : UInt64 := offsetBasis
    h := hashString h "module="
    h := hashString h moduleName
    h := hashString h "|schema="
    h := hashString h schemaName
    h := hashString h "|instance="
    h := hashString h instanceName
    h := hashString h "|relation="
    h := hashString h relationName
    h := hashString h "|fields="

    for (field, value) in fieldsInDeclOrder do
      h := hashString h field
      h := hashString h "="
      h := hashString h value
      h := hashString h ";"

    factIdPrefix ++ toHexFixed16 h

end Fnv1a

end Axiograph.Util
