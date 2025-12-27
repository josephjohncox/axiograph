-- Social Network as Higher Groupoid
--
-- Models social relationships using 2-categorical structure:
-- - 0-cells: People
-- - 1-morphisms: Relationships (friend, colleague, family)
-- - 2-morphisms: Transformations of relationships
--
-- Key insight: relationships can CHANGE, and the change itself
-- has structure. "We were friends, then colleagues, now friends again"
-- forms a PATH in the space of relationships.
--
-- HoTT enables:
-- - Tracking relationship evolution
-- - Proving relationship equivalences
-- - Reasoning about social dynamics

module SocialNetwork

schema SocialGraph:
  -- ==========================================================================
  -- 0-Cells: Agents
  -- ==========================================================================
  object Person
  object Organization
  object Community

  -- ==========================================================================
  -- 1-Morphisms: Relationships (edges with structure)
  -- ==========================================================================
  object RelationType
  
  -- A relationship between people is a typed edge
  relation Relationship(from: Person, to: Person, relType: RelationType)
  
  -- Organization membership
  relation MemberOf(person: Person, org: Organization)
  
  -- Community participation
  relation ParticipatesIn(person: Person, comm: Community)

  -- ==========================================================================
  -- 2-Morphisms: Relationship Transformations
  -- ==========================================================================
  object RelTransformation  -- A change in relationship

  -- Track how relationships evolve
  relation RelationshipPath(
    from: Person,
    to: Person,
    startRel: RelationType,
    endRel: RelationType,
    transform: RelTransformation,
    time: Time
  )
  
  -- Composition of transformations (paths compose)
  relation TransformCompose(
    t1: RelTransformation,
    t2: RelTransformation,
    result: RelTransformation
  )

  -- ==========================================================================
  -- Equivalence and Trust
  -- ==========================================================================
  object TrustLevel
  
  -- Trust is path-dependent: HOW you became trusted matters
  relation TrustPath(
    from: Person,
    to: Person,
    level: TrustLevel,
    witnesses: Community  -- Who vouches for this trust
  )
  
  -- Trust transitivity with attenuation
  relation TrustComposition(
    a: Person, b: Person, c: Person,
    trust_ab: TrustLevel,
    trust_bc: TrustLevel,
    trust_ac: TrustLevel
  )

  -- ==========================================================================
  -- Higher Paths: Equivalences of Relationship Histories
  -- ==========================================================================
  
  -- Two relationship histories might be "equivalent"
  -- (different paths to the same social state)
  relation HistoryEquivalence(
    from: Person,
    to: Person,
    path1: RelTransformation,
    path2: RelTransformation,
    witness: Text  -- Proof/justification of equivalence
  )

  -- Support types
  object Time
  object Text

theory SocialRules on SocialGraph:
  -- Relationships are symmetric in some types
  constraint symmetric Relationship where
    Relationship.relType in {Friend, Colleague, Sibling}

  -- Trust composition is well-defined
  constraint functional TrustComposition(a, b, c, trust_ab, trust_bc) -> TrustComposition.trust_ac

  -- History equivalence is itself an equivalence relation
  constraint symmetric HistoryEquivalence
  constraint transitive HistoryEquivalence

  -- Transformation composition is associative
  -- (a ; b) ; c ≡ a ; (b ; c)
  equation transform_assoc:
    TransformCompose(TransformCompose(a, b, ab), c, result) =
    TransformCompose(a, TransformCompose(b, c, bc), result)

instance SocialExample of SocialGraph:
  -- People
  Person = {Alice, Bob, Carol, Dave}
  Organization = {TechCorp, University}
  Community = {MakerSpace, BookClub, Neighborhood}

  -- Relationship types (form a category!)
  RelationType = {
    Stranger,      -- No relationship
    Acquaintance,  -- Weak tie
    Friend,        -- Strong tie
    CloseFriend,   -- Very strong tie
    Colleague,     -- Professional tie
    Family,        -- Kin tie
    Mentor         -- Asymmetric developmental tie
  }

  -- Current relationships
  Relationship = {
    (from=Alice, to=Bob, relType=Friend),
    (from=Alice, to=Carol, relType=Colleague),
    (from=Bob, to=Carol, relType=Acquaintance),
    (from=Carol, to=Dave, relType=Mentor)
  }

  -- Relationship transformations (the 2-morphisms!)
  RelTransformation = {
    Strengthen,    -- Acquaintance -> Friend
    Weaken,        -- Friend -> Acquaintance
    Formalize,     -- Friend -> Colleague (add professional aspect)
    Deformalize,   -- Colleague -> Friend
    DeepTrust,     -- Friend -> CloseFriend
    MeetIntro,     -- Stranger -> Acquaintance
    Drift          -- Any -> Stranger (relationship decay)
  }

  Time = {T0, T1, T2, T3}

  -- Relationship evolution paths
  -- Alice and Bob's friendship evolved: Stranger -> Acquaintance -> Friend
  RelationshipPath = {
    (from=Alice, to=Bob, startRel=Stranger, endRel=Acquaintance, transform=MeetIntro, time=T0),
    (from=Alice, to=Bob, startRel=Acquaintance, endRel=Friend, transform=Strengthen, time=T1)
  }

  -- Transformation composition
  -- MeetIntro ; Strengthen ≡ "became friends"
  TransformCompose = {
    (t1=MeetIntro, t2=Strengthen, result=BecameFriends),
    (t1=Strengthen, t2=DeepTrust, result=BecameClose),
    (t1=MeetIntro, t2=Formalize, result=BecameColleagues)
  }

  -- Trust levels
  TrustLevel = {None, Low, Medium, High, Complete}

  -- Trust paths (with witnesses)
  TrustPath = {
    (from=Alice, to=Bob, level=High, witnesses=BookClub),
    (from=Bob, to=Carol, level=Medium, witnesses=TechCorp),
    (from=Alice, to=Carol, level=Low, witnesses=MakerSpace)
  }

  -- Trust composition (transitive but attenuated)
  TrustComposition = {
    -- Alice trusts Bob (High), Bob trusts Carol (Medium)
    -- Therefore Alice trusts Carol through Bob (Low)
    (a=Alice, b=Bob, c=Carol, trust_ab=High, trust_bc=Medium, trust_ac=Low)
  }

  -- History equivalences (2-paths!)
  -- Two different paths to friendship are "the same" socially
  Text = {SameFriendship, DifferentRoute}

  HistoryEquivalence = {
    -- Met at work then became friends ≡ Met socially then became colleagues
    -- Both paths end at "colleague-friend" state
    (from=Alice, to=Carol,
     path1=WorkThenFriend,
     path2=FriendThenWork,
     witness=SameFriendship)
  }

