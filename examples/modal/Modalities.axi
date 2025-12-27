-- Modalities demo: epistemic + deontic + explicit tacit evidence
--
-- This module is intentionally *small* but semantically rich:
--
-- - Epistemic structure: Worlds + accessibility + propositions that hold at worlds.
-- - Deontic structure: Ideal worlds + obligations that are obligatory at a world.
-- - Tacit/approx structure: Evidence and belief confidence are explicit, not hidden.
-- - 2-morphisms / "paths between paths": alternative justifications can be related by
--   an explicit equivalence relation (`JustificationEquiv`).
--
-- Important: a certificate proves derivability from inputs, not truth of the inputs.
-- This example keeps “knowledge/belief/obligation” explicit so “unknown vs false”
-- can be modeled without silently assuming a closed world.

module Modalities

schema Modal:
  -- ==========================================================================
  -- Core epistemic objects
  -- ==========================================================================
  object Agent
  object World
  object Proposition

  -- A proposition holds at a world (extensional truth-at-world relation).
  relation Holds(world: World, prop: Proposition)

  -- Accessibility between worlds (what worlds are considered possible).
  --
  -- For a larger model, you would usually have an agent-indexed accessibility
  -- relation. Here we keep one accessibility relation to stay readable; the
  -- “agent view” lives in `Knows/Believes` which are asserted/derived facts.
  relation Accessible(from: World, to: World)

  -- Precomputed epistemic facts (proof-relevant workflows would emit certs).
  relation Knows(agent: Agent, prop: Proposition)
  relation Believes(agent: Agent, prop: Proposition, conf: Confidence)

  -- ==========================================================================
  -- Deontic objects (obligations / policies)
  -- ==========================================================================
  object Obligation
  object Policy
  object Text

  -- Deontic accessibility: ideal worlds for a given world (simple Kripke-style).
  relation Ideal(from: World, to: World)

  -- Precomputed obligations (again: can be certificate-backed in the future).
  relation Obligatory(world: World, obl: Obligation)

  relation PolicySays(policy: Policy, obl: Obligation, text: Text)

  -- ==========================================================================
  -- Evidence / tacit knowledge (explicit confidence + provenance)
  -- ==========================================================================
  object Evidence
  object Confidence

  relation EvidenceSupports(ev: Evidence, prop: Proposition, conf: Confidence)
  relation EvidenceText(ev: Evidence, text: Text)

  -- ==========================================================================
  -- "Paths between paths": alternative justifications are explicitly comparable
  -- ==========================================================================
  object JustificationPath

  -- A justification path is a structured record of an argument for a conclusion.
  relation Justifies(path: JustificationPath, agent: Agent, conclusion: Obligation)

  -- Two justifications can be considered equivalent (a 2-morphism).
  relation JustificationEquiv(path1: JustificationPath, path2: JustificationPath, witness: Text)

theory ModalRules on Modal:
  -- Accessibility is an equivalence relation (S5-like shape).
  constraint symmetric Accessible
  constraint transitive Accessible

  -- Ideal-world relation is transitive (idealization can be iterated).
  constraint transitive Ideal

  -- Evidence is functional at a given (ev, prop) pair.
  constraint key EvidenceSupports(ev, prop)

  -- Justification equivalence is an equivalence relation.
  constraint symmetric JustificationEquiv
  constraint transitive JustificationEquiv

instance ModalitiesDemo of Modal:
  Agent = {Alice, Bob}

  World = {W0, W1}
  Proposition = {SafeCutting, HighSpeedOk}

  Obligation = {UseLowRPM, UseCoolant}
  Policy = {ShopPolicy_v1}

  Confidence = {High, Medium, Low}

  Text = {
    Text_Policy_UseLowRPM,
    Text_Policy_UseCoolant,
    Text_Evidence_SensorChatter,
    Text_Justification_Equiv
  }

  Evidence = {PolicyDoc_0, SensorTrace_0}

  -- Epistemic model (toy):
  --
  -- In both worlds SafeCutting holds; HighSpeedOk only holds in W1.
  Holds = {
    (world=W0, prop=SafeCutting),
    (world=W1, prop=SafeCutting),
    (world=W1, prop=HighSpeedOk)
  }

  -- Accessibility: both worlds are mutually accessible (one equivalence class).
  Accessible = {
    (from=W0, to=W0),
    (from=W0, to=W1),
    (from=W1, to=W0),
    (from=W1, to=W1)
  }

  -- Knowledge/belief are explicit:
  -- - Alice knows SafeCutting (true in all accessible worlds).
  -- - Alice only believes HighSpeedOk with low confidence (true only in some worlds).
  Knows = {(agent=Alice, prop=SafeCutting)}
  Believes = {(agent=Alice, prop=HighSpeedOk, conf=Low)}

  -- Evidence pointers + confidence (tacit / approximate).
  EvidenceSupports = {
    (ev=PolicyDoc_0, prop=SafeCutting, conf=High),
    (ev=SensorTrace_0, prop=HighSpeedOk, conf=Low)
  }

  EvidenceText = {
    (ev=PolicyDoc_0, text=Text_Policy_UseLowRPM),
    (ev=SensorTrace_0, text=Text_Evidence_SensorChatter)
  }

  -- Deontic model (toy):
  --
  -- W1 is ideal relative to W0 (e.g. "idealized shop conditions").
  Ideal = {(from=W0, to=W1), (from=W1, to=W1)}

  Obligatory = {
    (world=W0, obl=UseLowRPM),
    (world=W0, obl=UseCoolant)
  }

  PolicySays = {
    (policy=ShopPolicy_v1, obl=UseLowRPM, text=Text_Policy_UseLowRPM),
    (policy=ShopPolicy_v1, obl=UseCoolant, text=Text_Policy_UseCoolant)
  }

  -- Two alternative "paths" to the same obligation:
  -- - directly from policy doc
  -- - indirectly from sensor observation (chatter implies “use lower RPM”)
  JustificationPath = {Path_Policy, Path_Sensor}

  Justifies = {
    (path=Path_Policy, agent=Alice, conclusion=UseLowRPM),
    (path=Path_Sensor, agent=Alice, conclusion=UseLowRPM)
  }

  JustificationEquiv = {
    (path1=Path_Policy, path2=Path_Sensor, witness=Text_Justification_Equiv)
  }

