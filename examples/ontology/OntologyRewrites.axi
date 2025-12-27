-- Ontology rewrite rules (first-class, certificate-addressable semantics)
--
-- This module demonstrates *domain-meaningful* rewrite rules that live in a
-- canonical `.axi` theory (not a hard-coded Rust enum):
--
-- - Definitional/compositional semantics:
--     Grandparent(a,c)  ≃  Parent(a,b) ; Parent(b,c)
--
-- - Inverse/adjunction-like semantics:
--     ManagerOf(m,e)  ≃  (ReportsTo(e,m))⁻¹
--
-- These rules are intended to be referenced by certificates as:
--
--   axi:fnv1a64:<hex>:OrgFamilySemantics:<rule_name>
--
-- so that the trusted Lean checker can load the anchored `.axi` and validate
-- that each rewrite step is a correct application of the accepted ontology
-- semantics.

module OntologyRewrites

schema OrgFamily:
  object Person

  -- Family-like relations.
  relation Parent(parent: Person, child: Person)
  relation Grandparent(grandparent: Person, grandchild: Person)

  -- Organization-like relations.
  relation ReportsTo(employee: Person, manager: Person)
  relation ManagerOf(manager: Person, employee: Person)

theory OrgFamilySemantics on OrgFamily:
  -- (Optional) “shape” constraints. These are *data constraints* that can be
  -- indexed and used during planning/validation.
  constraint key Parent(parent, child)
  constraint functional ReportsTo.employee -> ReportsTo.manager

  -- --------------------------------------------------------------------------
  -- First-class rewrite rules
  -- --------------------------------------------------------------------------

  -- Grandparent is definitional as “parent of a parent”.
  --
  -- In path terms:
  --   Parent(a,b) ; Parent(b,c)  ↔  Grandparent(a,c)
  rewrite grandparent_def:
    orientation: bidirectional
    vars: a: Person, b: Person, c: Person
    lhs: trans(step(a, Parent, b), step(b, Parent, c))
    rhs: step(a, Grandparent, c)

  -- ManagerOf is defined as the inverse edge of ReportsTo.
  --
  -- In path terms:
  --   ManagerOf(m,e)  ↔  (ReportsTo(e,m))⁻¹
  rewrite manager_inverse_reports_to:
    orientation: bidirectional
    vars: e: Person, m: Person
    lhs: step(m, ManagerOf, e)
    rhs: inv(step(e, ReportsTo, m))

instance TinyOrgFamily of OrgFamily:
  Person = {Alice, Bob, Carol, Eve}

  Parent = {
    (parent=Alice, child=Bob),
    (parent=Bob, child=Carol)
  }

  -- This explicit fact is consistent with `grandparent_def`:
  --   Alice -Parent-> Bob -Parent-> Carol.
  Grandparent = {
    (grandparent=Alice, grandchild=Carol)
  }

  ReportsTo = {
    (employee=Eve, manager=Bob)
  }

  -- This explicit fact is consistent with `manager_inverse_reports_to`.
  ManagerOf = {
    (manager=Bob, employee=Eve)
  }

