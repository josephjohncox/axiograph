-- Schema Evolution via Univalence
--
-- Uses HoTT's univalence principle for ontology migration:
-- - Equivalent schemas are "the same" (can substitute)
-- - Schema changes that preserve structure are equivalences
-- - Data migration is transport along paths
--
-- This is the category-theoretic approach to schema evolution:
-- schemas are objects, migrations are morphisms,
-- equivalences are isomorphisms.

module SchemaEvolution

schema OntologyMeta:
  -- ==========================================================================
  -- Schemas as Objects
  -- ==========================================================================
  object Schema_        -- A schema (ontology definition)
  object Version        -- Version identifier
  object Timestamp

  relation SchemaVersion(schema: Schema_, version: Version, timestamp: Timestamp)

  -- ==========================================================================
  -- Schema Morphisms (Migrations)
  -- ==========================================================================
  object Migration      -- A migration between schemas

  relation MigrationFrom(migration: Migration, source: Schema_)
  relation MigrationTo(migration: Migration, target: Schema_)

  -- Migration composition (category structure)
  relation MigrationCompose(m1: Migration, m2: Migration, result: Migration)

  -- ==========================================================================
  -- Schema Equivalences (Univalence!)
  -- ==========================================================================
  -- Two schemas are equivalent if there's a structure-preserving bijection
  
  relation SchemaEquiv(
    s1: Schema_,
    s2: Schema_,
    forward: Migration,
    backward: Migration,
    proof: EquivProof
  )

  object EquivProof  -- Evidence of equivalence

  -- If schemas are equivalent, we can substitute one for the other
  -- This is the univalence axiom for ontologies!

  -- ==========================================================================
  -- Data Transport
  -- ==========================================================================
  object Instance_      -- An instance of a schema
  object DataMigration  -- Concrete data transformation

  relation InstanceOf(instance: Instance_, schema: Schema_)
  relation MigrateData(migration: Migration, sourceData: Instance_, targetData: Instance_)

  -- Transport preserves validity: if i : Instance_(S1) and S1 ≃ S2,
  -- then transport(i) : Instance_(S2)

  -- ==========================================================================
  -- Structural Changes (Path Constructors)
  -- ==========================================================================
  object ChangeType

  relation MigrationChanges(migration: Migration, changeType: ChangeType)

  -- Changes form a group: each change has an inverse
  relation ChangeInverse(change: ChangeType, inverse: ChangeType)

  -- ==========================================================================
  -- Coherence (Higher Laws)
  -- ==========================================================================
  -- Composition of equivalences is an equivalence
  -- Inverse of equivalence is an equivalence
  -- These are the groupoid laws!

  relation EquivCompose(e1: SchemaEquiv, e2: SchemaEquiv, result: SchemaEquiv)

  -- Support types
  object Text

theory EvolutionLaws on OntologyMeta:
  -- Migrations are functional (deterministic)
  constraint functional MigrationFrom.migration -> MigrationFrom.source
  constraint functional MigrationTo.migration -> MigrationTo.target

  -- Equivalence is symmetric (via backward migration)
  constraint symmetric SchemaEquiv

  -- Equivalence is transitive (via composition)
  constraint transitive SchemaEquiv

  -- Migration composition is associative
  equation migration_assoc:
    MigrationCompose(MigrationCompose(a, b, ab), c, result) =
    MigrationCompose(a, MigrationCompose(b, c, bc), result)

  -- Identity migration exists
  equation migration_identity:
    MigrationCompose(m, IdentityMigration, m) = m

  -- Inverse law for equivalences
  equation equiv_inverse:
    MigrationCompose(forward, backward, IdentityMigration) =
    SchemaEquiv.forward ; SchemaEquiv.backward

  -- Changes and their inverses
  constraint functional ChangeInverse.change -> ChangeInverse.inverse

instance ProductCatalog of OntologyMeta:
  -- Example: Evolution of a product catalog schema

  Schema_ = {
    ProductV1,      -- Original: just products
    ProductV2,      -- Added: categories
    ProductV3,      -- Normalized: separate SKU table
    ProductV3_alt,  -- Alternative normalization
    ProductV4       -- Denormalized for performance
  }

  Version = {V1, V2, V3, V3a, V4}
  Timestamp = {T2020, T2021, T2022, T2023}

  SchemaVersion = {
    (schema=ProductV1, version=V1, timestamp=T2020),
    (schema=ProductV2, version=V2, timestamp=T2021),
    (schema=ProductV3, version=V3, timestamp=T2022),
    (schema=ProductV3_alt, version=V3a, timestamp=T2022),
    (schema=ProductV4, version=V4, timestamp=T2023)
  }

  -- Migrations (the 1-morphisms)
  Migration = {
    AddCategories,      -- V1 -> V2
    NormalizeSKU,       -- V2 -> V3
    NormalizeAlt,       -- V2 -> V3_alt
    Denormalize,        -- V3 -> V4
    MergeCategories,    -- V2 -> V1 (inverse of AddCategories)
    JoinSKU,            -- V3 -> V2 (inverse of NormalizeSKU)
    IdentityMigration,  -- No-op
    V3toV3alt,          -- V3 -> V3_alt (equivalence!)
    V3altToV3           -- V3_alt -> V3
  }

  MigrationFrom = {
    (migration=AddCategories, source=ProductV1),
    (migration=NormalizeSKU, source=ProductV2),
    (migration=NormalizeAlt, source=ProductV2),
    (migration=V3toV3alt, source=ProductV3),
    (migration=V3altToV3, source=ProductV3_alt),
    (migration=Denormalize, source=ProductV3),
    (migration=MergeCategories, source=ProductV2),
    (migration=JoinSKU, source=ProductV3)
  }

  MigrationTo = {
    (migration=AddCategories, target=ProductV2),
    (migration=NormalizeSKU, target=ProductV3),
    (migration=NormalizeAlt, target=ProductV3_alt),
    (migration=V3toV3alt, target=ProductV3_alt),
    (migration=V3altToV3, target=ProductV3),
    (migration=Denormalize, target=ProductV4),
    (migration=MergeCategories, target=ProductV1),
    (migration=JoinSKU, target=ProductV2)
  }

  -- Change types
  ChangeType = {
    AddTable,      -- Add new table/object
    DropTable,     -- Remove table/object
    AddColumn,     -- Add attribute
    DropColumn,    -- Remove attribute
    Normalize,     -- Split table
    Denormalize_,  -- Join tables
    Rename,        -- Rename object
    TypeChange     -- Change attribute type
  }

  MigrationChanges = {
    (migration=AddCategories, changeType=AddTable),
    (migration=NormalizeSKU, changeType=Normalize),
    (migration=Denormalize, changeType=Denormalize_),
    (migration=V3toV3alt, changeType=Rename)  -- Just different naming
  }

  -- Change inverses (groupoid structure!)
  ChangeInverse = {
    (change=AddTable, inverse=DropTable),
    (change=AddColumn, inverse=DropColumn),
    (change=Normalize, inverse=Denormalize_),
    (change=Rename, inverse=Rename)  -- Self-inverse!
  }

  -- SCHEMA EQUIVALENCES (the key HoTT insight!)
  EquivProof = {
    IsoProof,           -- Full isomorphism
    LosslessProof,      -- Information-preserving
    SemanticEquiv       -- Same meaning, different structure
  }

  SchemaEquiv = {
    -- V3 ≃ V3_alt (two normalizations are equivalent!)
    (s1=ProductV3, s2=ProductV3_alt,
     forward=V3toV3alt, backward=V3altToV3,
     proof=IsoProof),

    -- AddCategories has inverse MergeCategories
    -- So V1 ≃ V2 (if we don't care about category info)
    -- This is a WEAKER equivalence (lossy backward)
  }

  -- Composition of migrations
  MigrationCompose = {
    -- AddCategories ; NormalizeSKU = direct V1->V3 migration
    (m1=AddCategories, m2=NormalizeSKU, result=DirectV1toV3),

    -- V3toV3alt ; V3altToV3 = Identity (equivalence roundtrip)
    (m1=V3toV3alt, m2=V3altToV3, result=IdentityMigration),

    -- V3altToV3 ; V3toV3alt = Identity
    (m1=V3altToV3, m2=V3toV3alt, result=IdentityMigration)
  }

  -- Instances and data migration
  Instance_ = {
    Products_Jan2020,   -- V1 instance
    Products_Jun2021,   -- V2 instance
    Products_Jan2023,   -- V3 instance
    Products_Jan2023_migrated  -- V3_alt instance (migrated from V3)
  }

  InstanceOf = {
    (instance=Products_Jan2020, schema=ProductV1),
    (instance=Products_Jun2021, schema=ProductV2),
    (instance=Products_Jan2023, schema=ProductV3),
    (instance=Products_Jan2023_migrated, schema=ProductV3_alt)
  }

  -- Data transport: migrating actual data along schema evolution
  DataMigration = {TransformJan2023}

  MigrateData = {
    -- Transport Products_Jan2023 along V3→V3_alt equivalence
    (migration=V3toV3alt,
     sourceData=Products_Jan2023,
     targetData=Products_Jan2023_migrated)
  }

  Text = {Proof_V3_equiv_V3alt}

