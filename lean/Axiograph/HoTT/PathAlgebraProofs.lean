import Axiograph.HoTT.KnowledgeGraph
import Axiograph.Prob.Verified

-- =============================================================================
-- Axiograph.HoTT.PathAlgebraProofs - Path algebra (Lean port)
-- =============================================================================
--
-- This module is a minimal, auditable port of
-- `idris/Axiograph/HoTT/PathAlgebraProofs.idr`.
--
-- It focuses on the parts needed for certificate-checking:
-- - path length witnesses and computation
-- - groupoid laws as `KGPathEquiv` constructors
-- - confidence accumulation along paths (fixed-point `VProb`)

namespace Axiograph.HoTT

open Axiograph.Prob

universe u

-- -----------------------------------------------------------------------------
-- Path Length as Indexed Type
-- -----------------------------------------------------------------------------

inductive PathLen {n : Nat} {kg : KnowledgeGraph.{u} n} :
    {a b : EntityId n} → KGPath kg a b → Nat → Type u where
  | LenRefl {a : EntityId n} :
      PathLen (a := a) (b := a) (KGPath.KGRefl (kg := kg) (a := a)) 0
  | LenRel {a b : EntityId n} (r : kg.Rel a b) :
      PathLen (a := a) (b := b) (KGPath.KGRel (kg := kg) r) 1
  | LenTrans {a b c : EntityId n} {p : KGPath kg a b} {q : KGPath kg b c} {lp lq : Nat} :
      PathLen (a := a) (b := b) p lp →
      PathLen (a := b) (b := c) q lq →
      PathLen (a := a) (b := c) (KGPath.KGTrans (kg := kg) p q) (lp + lq)

def computeLen {n : Nat} {kg : KnowledgeGraph.{u} n} :
    {a b : EntityId n} → (p : KGPath kg a b) → Σ len : Nat, PathLen (a := a) (b := b) p len
  | a, _, KGPath.KGRefl => ⟨0, PathLen.LenRefl (kg := kg) (a := a)⟩
  | a, b, KGPath.KGRel r => ⟨1, PathLen.LenRel (kg := kg) (a := a) (b := b) r⟩
  | a, c, KGPath.KGTrans p q =>
      let lp := computeLen (kg := kg) (a := a) (b := _) p
      let lq := computeLen (kg := kg) (a := _) (b := c) q
      ⟨lp.1 + lq.1, PathLen.LenTrans (kg := kg) (a := a) (b := _) (c := c) (p := p) (q := q) lp.2 lq.2⟩

def pathLen {n : Nat} {kg : KnowledgeGraph.{u} n} {a b : EntityId n} (p : KGPath kg a b) : Nat :=
  (computeLen (kg := kg) (a := a) (b := b) p).1

-- -----------------------------------------------------------------------------
-- Groupoid Laws (as 2-cells)
-- -----------------------------------------------------------------------------

def leftIdentity {n : Nat} {kg : KnowledgeGraph.{u} n} {a b : EntityId n} (p : KGPath kg a b) :
    KGPathEquiv (KGPath.KGTrans KGPath.KGRefl p) p :=
  KGPathEquiv.KGPEIdL

def rightIdentity {n : Nat} {kg : KnowledgeGraph.{u} n} {a b : EntityId n} (p : KGPath kg a b) :
    KGPathEquiv (KGPath.KGTrans p KGPath.KGRefl) p :=
  KGPathEquiv.KGPEIdR

def associativity {n : Nat} {kg : KnowledgeGraph.{u} n} {a b c d : EntityId n} (p : KGPath kg a b)
    (q : KGPath kg b c) (r : KGPath kg c d) :
    KGPathEquiv (KGPath.KGTrans (KGPath.KGTrans p q) r) (KGPath.KGTrans p (KGPath.KGTrans q r)) :=
  KGPathEquiv.KGPEAssoc

-- -----------------------------------------------------------------------------
-- Confidence as Indexed Type
-- -----------------------------------------------------------------------------

inductive PathConf {n : Nat} {kg : KnowledgeGraph.{u} n}
    (getConf : {a b : EntityId n} → kg.Rel a b → VProb) :
    {a b : EntityId n} → KGPath kg a b → VProb → Type u where
  | PCRefl {a : EntityId n} :
      PathConf getConf (a := a) (b := a) (KGPath.KGRefl (kg := kg) (a := a)) vOne
  | PCRel {a b : EntityId n} (r : kg.Rel a b) :
      PathConf getConf (a := a) (b := b) (KGPath.KGRel (kg := kg) r) (getConf r)
  | PCTrans {a b c : EntityId n} {p : KGPath kg a b} {q : KGPath kg b c} {cp cq : VProb} :
      PathConf getConf (a := a) (b := b) p cp →
      PathConf getConf (a := b) (b := c) q cq →
      PathConf getConf (a := a) (b := c) (KGPath.KGTrans (kg := kg) p q) (vMult cp cq)

def computeConf {n : Nat} {kg : KnowledgeGraph.{u} n}
    (getConf : {a b : EntityId n} → kg.Rel a b → VProb) :
    {a b : EntityId n} → (p : KGPath kg a b) → Σ conf : VProb, PathConf (kg := kg) getConf (a := a) (b := b) p conf
  | a, _, KGPath.KGRefl =>
      ⟨vOne, PathConf.PCRefl (kg := kg) (getConf := getConf) (a := a)⟩
  | a, b, KGPath.KGRel r =>
      ⟨getConf r, PathConf.PCRel (kg := kg) (getConf := getConf) (a := a) (b := b) r⟩
  | a, c, KGPath.KGTrans p q =>
      let cp := computeConf (kg := kg) getConf (a := a) (b := _) p
      let cq := computeConf (kg := kg) getConf (a := _) (b := c) q
      ⟨vMult cp.1 cq.1, PathConf.PCTrans (kg := kg) (getConf := getConf) (a := a) (b := _) (c := c) (p := p) (q := q) cp.2 cq.2⟩

def pathConf {n : Nat} {kg : KnowledgeGraph.{u} n}
    (getConf : {a b : EntityId n} → kg.Rel a b → VProb) {a b : EntityId n} (p : KGPath kg a b) : VProb :=
  (computeConf (kg := kg) getConf (a := a) (b := b) p).1

end Axiograph.HoTT
