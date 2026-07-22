import Std

/-!
# SHA-256 and Axiograph V2 identity framing

This module is intentionally self-contained so the trusted checker recomputes
cryptographic identities instead of accepting a Rust-supplied digest.

The V2 preimage is byte-exact:

1. ASCII magic `AXIOGRAPH-ID` (12 bytes),
2. version `2` as unsigned 16-bit big-endian,
3. ASCII domain length as unsigned 16-bit big-endian,
4. lowercase ASCII domain bytes,
5. field count as unsigned 32-bit big-endian,
6. each ordered field as unsigned 64-bit big-endian byte length followed by
   the exact bytes.

No newline or Unicode normalization is performed.
-/

namespace Axiograph.Util.Sha256

private def initialState : Array UInt32 := #[
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
  0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19
]

private def roundConstants : Array UInt32 := #[
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
  0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
  0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
  0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
  0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
  0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
  0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
  0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
  0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
]

private def rotateRight (value : UInt32) (amount : Nat) : UInt32 :=
  (value >>> UInt32.ofNat amount) ||| (value <<< UInt32.ofNat (32 - amount))

private def choose (x y z : UInt32) : UInt32 :=
  (x &&& y) ^^^ ((~~~x) &&& z)

private def majority (x y z : UInt32) : UInt32 :=
  (x &&& y) ^^^ (x &&& z) ^^^ (y &&& z)

private def bigSigma0 (x : UInt32) : UInt32 :=
  rotateRight x 2 ^^^ rotateRight x 13 ^^^ rotateRight x 22

private def bigSigma1 (x : UInt32) : UInt32 :=
  rotateRight x 6 ^^^ rotateRight x 11 ^^^ rotateRight x 25

private def smallSigma0 (x : UInt32) : UInt32 :=
  rotateRight x 7 ^^^ rotateRight x 18 ^^^ (x >>> 3)

private def smallSigma1 (x : UInt32) : UInt32 :=
  rotateRight x 17 ^^^ rotateRight x 19 ^^^ (x >>> 10)

private def pushUInt64BE (bytes : ByteArray) (value : UInt64) : ByteArray :=
  Id.run do
    let mut out := bytes
    for shift in [56, 48, 40, 32, 24, 16, 8, 0] do
      out := out.push <| UInt8.ofNat (((value >>> UInt64.ofNat shift) &&& 0xff).toNat)
    return out

private def pad (input : ByteArray) : ByteArray :=
  Id.run do
    let bitLength : UInt64 := UInt64.ofNat input.size * 8
    let mut out := input.push 0x80
    while out.size % 64 != 56 do
      out := out.push 0
    out := pushUInt64BE out bitLength
    return out

private def readUInt32BE (bytes : ByteArray) (offset : Nat) : UInt32 :=
  let b0 := UInt32.ofNat (bytes.get! offset).toNat
  let b1 := UInt32.ofNat (bytes.get! (offset + 1)).toNat
  let b2 := UInt32.ofNat (bytes.get! (offset + 2)).toNat
  let b3 := UInt32.ofNat (bytes.get! (offset + 3)).toNat
  (b0 <<< 24) ||| (b1 <<< 16) ||| (b2 <<< 8) ||| b3

private def pushUInt32BE (bytes : ByteArray) (value : UInt32) : ByteArray :=
  Id.run do
    let mut out := bytes
    for shift in [24, 16, 8, 0] do
      out := out.push <| UInt8.ofNat (((value >>> UInt32.ofNat shift) &&& 0xff).toNat)
    return out

/-- Compute the 32-byte SHA-256 digest of exact input bytes. -/
def hashBytes (input : ByteArray) : ByteArray :=
  Id.run do
    let message := pad input
    let mut state := initialState
    let mut chunkOffset := 0
    while chunkOffset < message.size do
      let mut schedule : Array UInt32 := Array.replicate 64 0
      for index in [0:16] do
        schedule := schedule.set! index (readUInt32BE message (chunkOffset + index * 4))
      for index in [16:64] do
        let word := smallSigma1 schedule[index - 2]!
          + schedule[index - 7]!
          + smallSigma0 schedule[index - 15]!
          + schedule[index - 16]!
        schedule := schedule.set! index word

      let mut a := state[0]!
      let mut b := state[1]!
      let mut c := state[2]!
      let mut d := state[3]!
      let mut e := state[4]!
      let mut f := state[5]!
      let mut g := state[6]!
      let mut h := state[7]!

      for index in [0:64] do
        let temp1 := h + bigSigma1 e + choose e f g
          + roundConstants[index]! + schedule[index]!
        let temp2 := bigSigma0 a + majority a b c
        h := g
        g := f
        f := e
        e := d + temp1
        d := c
        c := b
        b := a
        a := temp1 + temp2

      state := state.set! 0 (state[0]! + a)
      state := state.set! 1 (state[1]! + b)
      state := state.set! 2 (state[2]! + c)
      state := state.set! 3 (state[3]! + d)
      state := state.set! 4 (state[4]! + e)
      state := state.set! 5 (state[5]! + f)
      state := state.set! 6 (state[6]! + g)
      state := state.set! 7 (state[7]! + h)
      chunkOffset := chunkOffset + 64

    let mut digest := ByteArray.empty
    for word in state do
      digest := pushUInt32BE digest word
    return digest

private def hexDigit (n : Nat) : Char :=
  if n < 10 then Char.ofNat (n + '0'.toNat)
  else Char.ofNat (n - 10 + 'a'.toNat)

/-- Lowercase hexadecimal encoding. -/
def toHex (bytes : ByteArray) : String :=
  String.ofList <| bytes.foldl (init := []) fun chars byte =>
    chars ++ [hexDigit (byte.toNat / 16), hexDigit (byte.toNat % 16)]

private def validDomainBytes (bytes : ByteArray) : Bool :=
  if bytes.isEmpty then false
  else Id.run do
    for byte in bytes do
      let valid :=
        (byte >= 0x61 && byte <= 0x7a) ||
        (byte >= 0x30 && byte <= 0x39) ||
        byte == 0x2d
      if !valid then return false
    return true

private def pushNatBE (bytes : ByteArray) (width : Nat) (value : Nat) : ByteArray :=
  Id.run do
    let mut out := bytes
    for index in [0:width] do
      let shift := (width - index - 1) * 8
      out := out.push <| UInt8.ofNat ((value / (2 ^ shift)) % 256)
    return out

/-- Build the canonical domain-separated, length-framed V2 preimage. -/
def canonicalIdentityPreimageV2
    (domain : String) (fields : Array ByteArray) : Except String ByteArray := do
  let domainBytes := domain.toUTF8
  if !validDomainBytes domainBytes then
    throw "identity domain must be non-empty lowercase ASCII using letters, digits, or hyphens"
  if domainBytes.size > 0xffff then
    throw "identity domain is too long"
  if fields.size > 0xffffffff then
    throw "identity preimage contains too many fields"

  let mut out := "AXIOGRAPH-ID".toUTF8
  out := pushNatBE out 2 2
  out := pushNatBE out 2 domainBytes.size
  out := out ++ domainBytes
  out := pushNatBE out 4 fields.size
  for field in fields do
    if field.size > 0xffffffffffffffff then
      throw "identity preimage field is too large"
    out := pushNatBE out 8 field.size
    out := out ++ field
  return out

example : toHex (hashBytes ByteArray.empty) =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" := by
  native_decide

example : toHex (hashBytes "abc".toUTF8) =
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" := by
  native_decide

example : toHex (hashBytes
    "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".toUTF8) =
    "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1" := by
  native_decide

end Axiograph.Util.Sha256
