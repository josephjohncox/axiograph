import Axiograph.Util.Dec
import Axiograph.HoTT.Core
import Axiograph.Prob.Verified

-- =============================================================================
-- Axiograph.HoTT.KnowledgeGraph - HoTT-based knowledge graph core (Lean port)
-- =============================================================================
--
-- This module is a minimal, auditable port of
-- `idris/Axiograph/HoTT/KnowledgeGraph.idr`.
--
-- It defines:
-- - a dependent knowledge-graph interface (`KnowledgeGraph`)
-- - paths as explicit witnesses (`KGPath`)
-- - path equivalence as 2-cells (`KGPathEquiv`)

namespace Axiograph.HoTT

open Axiograph.Prob

universe u v

-- -----------------------------------------------------------------------------
-- Type-Level Entity Universe
-- -----------------------------------------------------------------------------

abbrev EntityId (n : Nat) : Type := Fin n
abbrev RelationId : Type := Nat

-- -----------------------------------------------------------------------------
-- Knowledge Graph as Dependent Record
-- -----------------------------------------------------------------------------

structure KnowledgeGraph (numEntities : Nat) : Type (u + 1) where
  Rel : EntityId numEntities → EntityId numEntities → Type u
  decRel : (a : EntityId numEntities) → (b : EntityId numEntities) → Axiograph.Dec (Rel a b)
  relComp : {a b c : EntityId numEntities} → Rel a b → Rel b c → Rel a c
  relId : (a : EntityId numEntities) → Rel a a

-- -----------------------------------------------------------------------------
-- Paths in Knowledge Graph
-- -----------------------------------------------------------------------------

inductive KGPath {n : Nat} (kg : KnowledgeGraph.{u} n) : EntityId n → EntityId n → Type u where
  | KGRefl {a : EntityId n} : KGPath kg a a
  | KGRel {a b : EntityId n} : kg.Rel a b → KGPath kg a b
  | KGTrans {a b c : EntityId n} : KGPath kg a b → KGPath kg b c → KGPath kg a c

-- Symmetry requires symmetric relations.
structure SymmetricRel {n : Nat} (kg : KnowledgeGraph.{u} n) : Type (u + 1) where
  sym : (a b : EntityId n) → kg.Rel a b → kg.Rel b a

def kgPathSym {n : Nat} {kg : KnowledgeGraph.{u} n} (s : SymmetricRel kg) :
    {a b : EntityId n} → KGPath kg a b → KGPath kg b a
  | _, _, .KGRefl => .KGRefl
  | _, _, .KGRel r => .KGRel (s.sym _ _ r)
  | _, _, .KGTrans p q => .KGTrans (kgPathSym s q) (kgPathSym s p)

-- -----------------------------------------------------------------------------
-- Path Equivalence (2-Cells / Homotopies)
-- -----------------------------------------------------------------------------

inductive KGPathEquiv {n : Nat} {kg : KnowledgeGraph.{u} n} :
    {a b : EntityId n} → KGPath kg a b → KGPath kg a b → Type u where
  | KGPERefl {a b : EntityId n} {p : KGPath kg a b} : KGPathEquiv p p
  | KGPESym {a b : EntityId n} {p q : KGPath kg a b} : KGPathEquiv p q → KGPathEquiv q p
  | KGPETrans {a b : EntityId n} {p q r : KGPath kg a b} : KGPathEquiv p q → KGPathEquiv q r → KGPathEquiv p r
  | KGPEIdL {a b : EntityId n} {p : KGPath kg a b} : KGPathEquiv (KGPath.KGTrans KGPath.KGRefl p) p
  | KGPEIdR {a b : EntityId n} {p : KGPath kg a b} : KGPathEquiv (KGPath.KGTrans p KGPath.KGRefl) p
  | KGPEAssoc {a b c d : EntityId n} {p : KGPath kg a b} {q : KGPath kg b c} {r : KGPath kg c d} :
      KGPathEquiv (KGPath.KGTrans (KGPath.KGTrans p q) r) (KGPath.KGTrans p (KGPath.KGTrans q r))

-- 3-cells: paths between path equivalences (Idris collapses higher cells here).
abbrev KGPath3 {n : Nat} {kg : KnowledgeGraph.{u} n} {a b : EntityId n}
    {p q : KGPath kg a b} (pe1 pe2 : KGPathEquiv p q) : Prop :=
  pe1 = pe2

-- -----------------------------------------------------------------------------
-- Facts with Derivation Proof (used by higher layers)
-- -----------------------------------------------------------------------------

structure Fact {n : Nat} (kg : KnowledgeGraph.{u} n) (subject : EntityId n) (contentType : Type v) :
    Type (max u v) where
  content : contentType
  confidence : VProb
  derivationSource : EntityId n
  derivationPath : KGPath kg derivationSource subject
  evidenceCount : Nat

theorem factConfBounded {n : Nat} {kg : KnowledgeGraph.{u} n} {s : EntityId n} {c : Type v}
    (f : Fact kg s c) : toNat f.confidence ≤ Precision :=
  numeratorBounded f.confidence

def transportFact {n : Nat} {kg : KnowledgeGraph.{u} n} {a b : EntityId n} {c : Type v}
    (p : KGPath kg a b) : Fact kg a c → Fact kg b c
  | f =>
      { content := f.content
        confidence := f.confidence
        derivationSource := f.derivationSource
        derivationPath := KGPath.KGTrans f.derivationPath p
        evidenceCount := f.evidenceCount }

theorem transportPreservesSource {n : Nat} {kg : KnowledgeGraph.{u} n} {a b : EntityId n} {c : Type v}
    (p : KGPath kg a b) (f : Fact kg a c) :
    (transportFact (kg := kg) (a := a) (b := b) (c := c) p f).derivationSource = f.derivationSource :=
  rfl

end Axiograph.HoTT
