-- Family Structure as Homotopy Type
--
-- Models family relationships with HoTT structure:
-- - Paths represent kinship relations
-- - Path composition models relation chains (aunt = parent's sibling)
-- - Higher paths model "same relationship, different derivation"
-- - Univalence: equivalent family structures are interchangeable
--
-- Key insight: "cousin" can be derived multiple ways
-- (mother's sister's child = father's brother's child in terms of degree)
-- These are PATHS BETWEEN PATHS (2-paths / homotopies)

module FamilyHoTT

schema Family:
  -- ==========================================================================
  -- Persons (Points in the Space)
  -- ==========================================================================
  object Person
  object Gender
  object Generation  -- Relative generation (0 = self, 1 = parent, -1 = child)

  relation PersonGender(person: Person, gender: Gender)
  relation PersonGeneration(person: Person, gen: Generation)

  -- ==========================================================================
  -- Kinship Paths (The Type-Theoretic Core)
  -- ==========================================================================
  -- A kinship relation is a PATH between persons
  -- Think: Path(Person, Person) but enriched with relation type
  
  object KinshipType
  
  -- Primitive kinship paths (generators)
  relation Parent(child: Person, parent: Person)  -- path from child to parent
  relation Spouse(p1: Person, p2: Person)         -- symmetric path
  
  -- Derived kinship (composed paths)
  relation Kinship(from: Person, to: Person, relType: KinshipType, derivation: PathDerivation)

  -- ==========================================================================
  -- Path Composition (Building Complex Relations)
  -- ==========================================================================
  object PathDerivation  -- Records HOW a relationship was derived

  -- Composition of kinship types (category structure)
  relation KinshipCompose(r1: KinshipType, r2: KinshipType, result: KinshipType)

  -- Example: Parent ∘ Parent = Grandparent
  --          Sibling ∘ Parent = Aunt/Uncle

  -- ==========================================================================
  -- Path Equivalences (Multiple Derivations)
  -- ==========================================================================
  -- Two paths to the same kinship relation
  
  relation PathEquivalence(
    from: Person,
    to: Person,
    path1: PathDerivation,
    path2: PathDerivation,
    relType: KinshipType
  )

  -- Example: cousin via mother's side ≡ cousin via father's side (same degree)

  -- ==========================================================================
  -- Genealogical Distance (Metric on Paths)
  -- ==========================================================================
  object Degree  -- Kinship degree (number of steps)
  object Removal -- Generational difference

  relation KinshipDegree(relType: KinshipType, degree: Degree, removal: Removal)

  -- The degree is a homotopy invariant: 
  -- equivalent paths have same degree

  -- ==========================================================================
  -- Legal/Social Equivalences
  -- ==========================================================================
  -- Different cultures have different kinship equivalences!
  -- This is like choosing a homotopy type theory
  
  object CultureContext
  
  relation CulturalEquivalence(
    culture: CultureContext,
    rel1: KinshipType,
    rel2: KinshipType,
    reason: Text
  )

  -- Example: In some cultures, parallel cousins ≡ siblings (for marriage rules)

  -- Support types
  object Text

theory KinshipLaws on Family:
  -- Parent is functional (unique parents, barring adoption)
  constraint key Parent(child)

  -- Spouse is symmetric
  constraint symmetric Spouse

  -- Path equivalence is an equivalence relation
  constraint symmetric PathEquivalence
  constraint transitive PathEquivalence

  -- Composition is associative (category law)
  equation kinship_assoc:
    KinshipCompose(KinshipCompose(a, b, ab), c, result) =
    KinshipCompose(a, KinshipCompose(b, c, bc), result)

  -- Identity: Self ∘ R = R
  equation kinship_identity:
    KinshipCompose(Self, r, r) = r

  -- Degree is well-defined: equivalent paths have same degree
  constraint functional KinshipDegree.relType -> KinshipDegree.degree

  -- Cultural equivalences are per-culture
  constraint key CulturalEquivalence(culture, rel1, rel2)

instance ExtendedFamily of Family:
  -- Persons
  Person = {
    -- Generation 0 (focus)
    Alice, Bob,
    -- Generation 1 (parents)
    Charles, Diana, Edward, Fiona,
    -- Generation 2 (grandparents)
    George, Helen, Ivan, Julia,
    -- Generation -1 (children)
    Kevin, Lisa, Mike
  }

  Gender = {Male, Female, NonBinary}
  Generation = {Gen_Minus2, Gen_Minus1, Gen_0, Gen_Plus1, Gen_Plus2}

  PersonGender = {
    (person=Alice, gender=Female),
    (person=Bob, gender=Male),
    (person=Charles, gender=Male),
    (person=Diana, gender=Female),
    (person=Kevin, gender=Male)
  }

  -- Kinship types (these are the path types!)
  KinshipType = {
    -- Primitive
    Self,
    Parent_, Child_,
    Spouse_,
    
    -- First-order derived
    Sibling,        -- Parent⁻¹ ∘ Parent (share parent)
    Grandparent, Grandchild,
    
    -- Second-order derived
    Aunt, Uncle, Niece, Nephew,
    Cousin,
    GreatGrandparent, GreatGrandchild,
    
    -- In-law relations (via spouse paths)
    ParentInLaw, ChildInLaw,
    SiblingInLaw,
    
    -- Step relations (via spouse then parent)
    StepParent, StepChild, StepSibling
  }

  -- Primitive relationships (generators)
  Parent = {
    (child=Alice, parent=Charles),
    (child=Alice, parent=Diana),
    (child=Bob, parent=Edward),
    (child=Bob, parent=Fiona),
    (child=Charles, parent=George),
    (child=Charles, parent=Helen),
    (child=Diana, parent=Ivan),
    (child=Diana, parent=Julia),
    (child=Kevin, parent=Alice),
    (child=Kevin, parent=Bob)
  }

  Spouse = {
    (p1=Alice, p2=Bob),
    (p1=Charles, p2=Diana),  -- Assume blended family
    (p1=George, p2=Helen)
  }

  -- Kinship composition rules (the category structure!)
  KinshipCompose = {
    -- Parent ∘ Parent = Grandparent
    (r1=Parent_, r2=Parent_, result=Grandparent),
    
    -- Child ∘ Child = Grandchild
    (r1=Child_, r2=Child_, result=Grandchild),
    
    -- Parent⁻¹ ∘ Parent = Sibling (share a parent)
    (r1=Child_, r2=Parent_, result=Sibling),
    
    -- Sibling ∘ Parent = Aunt/Uncle
    (r1=Sibling, r2=Parent_, result=Aunt),  -- or Uncle based on gender
    
    -- Child ∘ Sibling = Niece/Nephew
    (r1=Child_, r2=Sibling, result=Nephew),  -- or Niece
    
    -- Cousin = Child ∘ Aunt (child of aunt/uncle)
    (r1=Child_, r2=Aunt, result=Cousin),
    
    -- In-laws via spouse
    (r1=Spouse_, r2=Parent_, result=ParentInLaw),
    (r1=Spouse_, r2=Sibling, result=SiblingInLaw)
  }

  -- Path derivations (how we computed the relationship)
  PathDerivation = {
    -- Primitive
    DirectParent, DirectSpouse,
    
    -- Via mother
    ViaMother,
    -- Via father
    ViaFather,
    
    -- Combined paths
    MothersSibling, FathersSibling,
    MothersParent, FathersParent
  }

  Degree = {D0, D1, D2, D3, D4}  -- 0 = self, 1 = parent/child/sibling, etc.
  Removal = {R0, R1, R2}  -- 0 = same generation, 1 = one apart, etc.

  KinshipDegree = {
    (relType=Self, degree=D0, removal=R0),
    (relType=Parent_, degree=D1, removal=R1),
    (relType=Sibling, degree=D2, removal=R0),  -- 2 steps (up then down)
    (relType=Grandparent, degree=D2, removal=R2),
    (relType=Aunt, degree=D3, removal=R1),
    (relType=Cousin, degree=D4, removal=R0)    -- 4 steps
  }

  -- PATH EQUIVALENCES (the higher structure!)
  -- Two ways to derive "cousin" are equivalent
  PathEquivalence = {
    -- Cousin via mother's side ≡ cousin via father's side (same degree!)
    (from=Alice, to=Bob, path1=MothersCousinPath, path2=FathersCousinPath, relType=Cousin),
    
    -- Two paths to grandparent
    (from=Kevin, to=George, path1=ThroughCharles, path2=ThroughDiana, relType=Grandparent)
  }

  -- Cultural contexts (different "homotopy theories"!)
  CultureContext = {Western, Arabic, Chinese, Hawaiian}

  Text = {ParallelEquiv, CrossEquiv, GenerationalResp}

  -- Cultural kinship equivalences
  CulturalEquivalence = {
    -- In Hawaiian kinship: all cousins ≡ siblings
    (culture=Hawaiian, rel1=Cousin, rel2=Sibling, reason=HawaiianKin),
    
    -- In some systems: parallel cousins ≡ siblings (for marriage taboos)
    (culture=Arabic, rel1=ParallelCousin, rel2=Sibling, reason=ParallelEquiv)
  }

