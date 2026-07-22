-- Family ontology example
-- Small canonical `axi_v1` module with explicit context/time roles.

module Family

schema Fam:
  object Person
  object Context
  object Time

  -- Parent relation: (child, parent) scoped by context and time.
  relation Parent(child: Person, parent: Person, ctx: Context @context, time: Time @temporal)

  -- Spouse relation: symmetric and context-scoped.
  relation Spouse(a: Person, b: Person, ctx: Context @context)

  -- Sibling relation: symmetric
  relation Sibling(a: Person, b: Person)

theory FamRules on Fam:
  -- Each person has at most two parents (per context/time).
  constraint at_most 2 Parent.child -> Parent.parent param (ctx, time)

  -- Spouse is symmetric
  constraint symmetric Spouse

  -- Sibling is symmetric
  constraint symmetric Sibling

  -- Key constraints
  constraint key Parent(child, parent, ctx, time)
  constraint key Spouse(a, b, ctx)

instance TinyFamily of Fam:
  Person = {Alice, Bob, Carol, Dan, Eve, Frank}
  Context = {CensusData, FamilyTree}
  Time = {T2020, T2023}

  Parent = {
    (child=Carol, parent=Alice, ctx=CensusData, time=T2020),
    (child=Carol, parent=Bob, ctx=CensusData, time=T2020),
    (child=Dan, parent=Alice, ctx=CensusData, time=T2020),
    (child=Dan, parent=Bob, ctx=CensusData, time=T2020),
    (child=Eve, parent=Carol, ctx=FamilyTree, time=T2023),
    (child=Eve, parent=Frank, ctx=FamilyTree, time=T2023)
  }

  Spouse = {
    (a=Alice, b=Bob, ctx=CensusData),
    (a=Bob, b=Alice, ctx=CensusData),
    (a=Carol, b=Frank, ctx=FamilyTree),
    (a=Frank, b=Carol, ctx=FamilyTree)
  }

  Sibling = {
    (a=Carol, b=Dan),
    (a=Dan, b=Carol)
  }
