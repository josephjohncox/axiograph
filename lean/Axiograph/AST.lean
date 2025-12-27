import Std

-- =============================================================================
-- Axiograph.AST - Serializable surface AST (Lean port)
-- =============================================================================
--
-- Direct port of `idris/Axiograph/AST.idr`.
-- This is the named, serializable representation used for interchange and
-- (later) `.axi` parsing in the trusted Lean checker.

namespace Axiograph

abbrev Name : Type := String

-- -----------------------------------------------------------------------------
-- Schema AST
-- -----------------------------------------------------------------------------

structure FieldDecl where
  field : Name
  ty : Name
deriving Repr, DecidableEq

structure RelationDecl where
  name : Name
  fields : List FieldDecl
  context : Option Name
  temporal : Option Name
deriving Repr, DecidableEq

structure ArrowDecl where
  name : Name
  src : Name
  dst : Name
deriving Repr, DecidableEq

structure SubtypeDecl where
  sub : Name
  sup : Name
  incl : Name
deriving Repr, DecidableEq

structure PathAST where
  gens : List Name
deriving Repr, DecidableEq

structure EquationDecl where
  lhs : PathAST
  rhs : PathAST
deriving Repr, DecidableEq

structure SchemaAST where
  name : Name
  objects : List Name
  arrows : List ArrowDecl
  subtypes : List SubtypeDecl
  relations : List RelationDecl
  equations : List EquationDecl
deriving Repr, DecidableEq

-- -----------------------------------------------------------------------------
-- Constraints / Theory
-- -----------------------------------------------------------------------------

inductive Constraint where
  | functional (rel srcField dstField : Name)
  | total (rel srcField : Name)
  | image (rel field subtypeIncl subtypeObj : Name)
  | disjoint (incl1 incl2 : Name)
  | key (rel : Name) (fields : List Name)
  | symmetric (rel fieldA fieldB : Name)
  | transitive (rel fieldA fieldB : Name)
  | inverse (rel1 a1 b1 rel2 a2 : Name)                 -- b2 implied
  | inverse2 (rel1 a1 b1 rel2 a2 b2 : Name)
  | reflexive (rel field : Name)
  | antiSymmetric (rel fieldA fieldB : Name)
  | irreflexive (rel field : Name)
deriving Repr, DecidableEq

structure TheoryAST where
  name : Name
  schema : Name
  constraints : List (Name × Constraint)
deriving Repr, DecidableEq

-- -----------------------------------------------------------------------------
-- Instance AST
-- -----------------------------------------------------------------------------

structure ObjElems where
  obj : Name
  elems : List Name
deriving Repr, DecidableEq

structure ArrowMapEntry where
  arrow : Name
  pairs : List (Name × Name)
deriving Repr, DecidableEq

structure RelTuple where
  fields : List (Name × Name)  -- (fieldName, value)
deriving Repr, DecidableEq

structure RelInstanceEntry where
  rel : Name
  tuples : List RelTuple
deriving Repr, DecidableEq

structure InstanceAST where
  name : Name
  schema : Name
  objects : List ObjElems
  arrows : List ArrowMapEntry
  relations : List RelInstanceEntry
deriving Repr, DecidableEq

-- -----------------------------------------------------------------------------
-- Warehouse bindings
-- -----------------------------------------------------------------------------

structure SourceRef where
  sourceName : Name
  tableName : Name
deriving Repr, DecidableEq

structure ObjBinding where
  obj : Name
  src : SourceRef
  idCol : Name
deriving Repr, DecidableEq

structure RelBinding where
  rel : Name
  src : SourceRef
  colMap : List (Name × Name) -- field -> column
deriving Repr, DecidableEq

structure WarehouseAST where
  name : Name
  schema : Name
  objects : List ObjBinding
  relations : List RelBinding
deriving Repr, DecidableEq

-- -----------------------------------------------------------------------------
-- Complete module AST
-- -----------------------------------------------------------------------------

structure ModuleAST where
  moduleName : Name
  schemas : List SchemaAST
  theories : List TheoryAST
  instances : List InstanceAST
  warehouses : List WarehouseAST
deriving Repr, DecidableEq

-- -----------------------------------------------------------------------------
-- Binary tags (for future stable binary interchange)
-- -----------------------------------------------------------------------------

inductive ASTTag where
  | schema
  | theory
  | instance
  | warehouse
  | constraint
  | relation
  | arrow
deriving Repr, DecidableEq

def tagToByte : ASTTag → UInt8
  | .schema => (0x01 : UInt8)
  | .theory => (0x02 : UInt8)
  | .instance => (0x03 : UInt8)
  | .warehouse => (0x04 : UInt8)
  | .constraint => (0x05 : UInt8)
  | .relation => (0x06 : UInt8)
  | .arrow => (0x07 : UInt8)

def byteToTag : UInt8 → Option ASTTag
  | 0x01 => some .schema
  | 0x02 => some .theory
  | 0x03 => some .instance
  | 0x04 => some .warehouse
  | 0x05 => some .constraint
  | 0x06 => some .relation
  | 0x07 => some .arrow
  | _ => none

-- -----------------------------------------------------------------------------
-- Pretty printing (minimal)
-- -----------------------------------------------------------------------------

def showPath : PathAST → String
  | ⟨[]⟩ => "id"
  | ⟨gs⟩ => String.intercalate " . " gs

def showConstraint : Constraint → String
  | .functional rel src dst => s!"functional {rel}.{src} -> {rel}.{dst}"
  | .total rel src => s!"total {rel}.{src}"
  | .image rel field incl obj => s!"image {rel}.{field} via {incl} : {obj}"
  | .disjoint i1 i2 => s!"disjoint {i1}, {i2}"
  | .key rel fields => s!"key {rel}({String.intercalate ", " fields})"
  | .symmetric rel a b => s!"symmetric {rel}.{a} <-> {rel}.{b}"
  | .transitive rel a b => s!"transitive {rel}.{a} -> {rel}.{b}"
  | .inverse rel1 a1 b1 rel2 a2 => s!"inverse {rel1}({a1},{b1}) ~ {rel2}({a2},_)"
  | .inverse2 rel1 a1 b1 rel2 a2 b2 => s!"inverse {rel1}({a1},{b1}) ~ {rel2}({a2},{b2})"
  | .reflexive rel field => s!"reflexive {rel}.{field}"
  | .antiSymmetric rel a b => s!"antisymmetric {rel}.{a} <-> {rel}.{b}"
  | .irreflexive rel field => s!"irreflexive {rel}.{field}"

end Axiograph
