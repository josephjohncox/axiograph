-- PhysicsOntology.axi
--
-- A “real-ish” physics ontology in canonical `axi_v1` format.
--
-- Design goals
-- ============
--
-- 1) Keep the *canonical* `.axi` language small and readable.
--    This file is meant to be:
--      - easy to review,
--      - easy to query,
--      - and stable enough to anchor certificates.
--
-- 2) Represent modern physics knowledge at multiple layers:
--      - mathematical structures (manifolds, forms, metrics, groups, algebras),
--      - physical theories (GR, QFT, gauge theory),
--      - and a learning/concept graph (prerequisites, guidelines, examples).
--
-- 3) Make tacit / approximate knowledge explicit.
--    We represent heuristics and “rules of thumb” as first-class objects with
--    explicit confidence; nothing here silently becomes ground truth.
--
-- What this is *not* (yet)
-- ========================
--
-- - A complete formalization of physics (that lives on the Lean side).
-- - A full dependent type system (we approximate dependencies with typed
--   relations + key/functional constraints, and we can later tighten this with
--   certificate-checked “well-typedness” proofs and rewrite derivations).
--
-- Naming conventions
-- ==================
--
-- - Identifiers are ASCII (current `axi_v1` surface): letters/digits/underscores.
-- - Stable identifiers use descriptive prefixes (e.g. `Metric_Minkowski`).
-- - Prose/formulas are referenced via `Text_*` identifiers (attach chunks/docs
--   for full content).

module PhysicsOntology

schema Physics:
  -- ==========================================================================
  -- Support types (lightweight “strings” + confidence)
  -- ==========================================================================
  object Text
  object Confidence

  -- ==========================================================================
  -- Learning / concept graph (extension structures)
  -- ==========================================================================
  --
  -- This uses the canonical learning vocabulary (`learning.rs`) so the REPL can
  -- extract a typed `LearningGraph`:
  --   - Concept prerequisites (`requires`)
  --   - Concept → guideline links (`explains`)
  --   - Example → concept links (`demonstrates`)
  --   - Concept description pointers (`conceptDescription`)
  object Concept
  object SafetyGuideline
  object Example

  relation requires(concept: Concept, prereq: Concept)
  relation explains(concept: Concept, guideline: SafetyGuideline)
  relation demonstrates(example: Example, concept: Concept)
  relation conceptDescription(concept: Concept, text: Text)
  relation exampleDescription(example: Example, text: Text)
  relation guidelineDescription(guideline: SafetyGuideline, text: Text)
  relation guidelineConfidence(guideline: SafetyGuideline, conf: Confidence)

  -- ==========================================================================
  -- Core math “atoms” (enough to express modern physics structures)
  -- ==========================================================================
  object Nat
  object ScalarFieldType      -- e.g. ℝ, ℂ
  object VectorSpace
  object InnerProductSpace
  subtype InnerProductSpace < VectorSpace
  object HilbertSpace
  subtype HilbertSpace < InnerProductSpace

  relation VectorSpaceOver(space: VectorSpace, field: ScalarFieldType)

  object LieGroup
  object LieAlgebra
  relation LieGroupHasLieAlgebra(group: LieGroup, alg: LieAlgebra)

  object Algebra
  object CliffordAlgebra
  subtype CliffordAlgebra < Algebra

  relation AlgebraOver(algebra: Algebra, field: ScalarFieldType)

  -- ==========================================================================
  -- Differential geometry: manifolds, metrics, connections, curvature, forms
  -- ==========================================================================
  object Manifold
  object SmoothManifold
  subtype SmoothManifold < Manifold
  object RiemannianManifold
  subtype RiemannianManifold < SmoothManifold
  object LorentzianManifold
  subtype LorentzianManifold < SmoothManifold
  object SymplecticManifold
  subtype SymplecticManifold < SmoothManifold

  relation ManifoldDimension(manifold: Manifold, dim: Nat)

  object Metric
  relation MetricOn(metric: Metric, manifold: SmoothManifold)

  object Connection
  relation LeviCivitaConnection(metric: Metric, connection: Connection)

  object CurvatureTensor
  relation CurvatureOf(connection: Connection, curvature: CurvatureTensor)

  object TensorField
  object VectorField
  subtype VectorField < TensorField
  relation TensorFieldOn(field: TensorField, manifold: SmoothManifold)

  object DifferentialForm
  object SymplecticForm
  subtype SymplecticForm < DifferentialForm

  relation FormOn(form: DifferentialForm, manifold: SmoothManifold)
  relation FormDegree(form: DifferentialForm, degree: Nat)

  -- Operations on forms (encoded as relations).
  relation ExteriorDerivative(input: DifferentialForm, output: DifferentialForm)
  relation Wedge(left: DifferentialForm, right: DifferentialForm, out: DifferentialForm)
  relation HodgeStar(metric: Metric, input: DifferentialForm, output: DifferentialForm)

  relation SymplecticFormOn(form: SymplecticForm, manifold: SymplecticManifold)
  relation SymplecticManifoldHasForm(manifold: SymplecticManifold, form: SymplecticForm)

  -- ==========================================================================
  -- Classical mechanics (symplectic/Hamiltonian)
  -- ==========================================================================
  object PhaseSpace
  subtype PhaseSpace < SymplecticManifold

  object Hamiltonian
  relation HamiltonianOn(H: Hamiltonian, phase: PhaseSpace)
  relation HamiltonianVectorFieldOf(H: Hamiltonian, X: VectorField)

  -- ==========================================================================
  -- Relativity (special + general)
  -- ==========================================================================
  object Spacetime
  subtype Spacetime < LorentzianManifold

  object StressEnergyTensor
  relation StressEnergyOn(T: StressEnergyTensor, spacetime: Spacetime)

  object EinsteinEquation
  relation EinsteinEquationOn(eq: EinsteinEquation, spacetime: Spacetime, stress: StressEnergyTensor)

  -- Clifford / spin structures (enough to connect to Dirac fields).
  relation CliffordAlgebraFromMetric(metric: Metric, alg: CliffordAlgebra)
  object GammaMatrix
  relation GammaMatrixInAlgebra(gamma: GammaMatrix, alg: CliffordAlgebra)

  -- ==========================================================================
  -- Quantum field theory (high level)
  -- ==========================================================================
  object QuantumFieldTheory
  object QuantumField
  object GaugeGroup
  subtype GaugeGroup < LieGroup

  object LagrangianDensity
  object ActionFunctional

  relation QFTOnSpacetime(qft: QuantumFieldTheory, spacetime: Spacetime)
  relation QFTHasField(qft: QuantumFieldTheory, field: QuantumField)
  relation QFTHasGaugeGroup(qft: QuantumFieldTheory, group: GaugeGroup)
  relation QFTHasLagrangian(qft: QuantumFieldTheory, lag: LagrangianDensity)
  relation LagrangianDefinesAction(lag: LagrangianDensity, action: ActionFunctional)

theory PhysicsRules on Physics:
  -- --------------------------------------------------------------------------
  -- Learning graph constraints (shallow invariants)
  -- --------------------------------------------------------------------------
  constraint key conceptDescription(concept, text)
  constraint key guidelineDescription(guideline, text)
  constraint key guidelineConfidence(guideline, conf)

  -- --------------------------------------------------------------------------
  -- Differential geometry invariants (small, checkable constraints)
  -- --------------------------------------------------------------------------
  constraint key ManifoldDimension(manifold, dim)
  constraint key MetricOn(metric, manifold)
  constraint key FormDegree(form, degree)
  constraint key FormOn(form, manifold)

  -- --------------------------------------------------------------------------
  -- Richer typing rules (stored as first-class *typing constraints*)
  -- --------------------------------------------------------------------------
  --
  -- These are examples of the kind of “dependent typing” we *want* to enforce:
  -- - d : Ω^k(M) → Ω^{k+1}(M)
  -- - wedge : Ω^k(M) × Ω^l(M) → Ω^{k+l}(M)
  -- - HodgeStar depends on (M, g, orientation) and maps degrees k ↔ n-k
  --
  -- Today the canonical surface records these as explicit constraints (reviewable),
  -- and the runtime can use them as metadata; later we can:
  -- - make these executable as part of typed elaboration / query planning, and
  -- - emit certificates that Lean checks against a formal semantics.
  constraint typing ExteriorDerivative: preserves_manifold_and_increments_degree
  constraint typing Wedge: preserves_manifold_and_adds_degree
  constraint typing HodgeStar: depends_on_metric_and_dualizes_degree

instance PhysicsPrimer of Physics:
  -- --------------------------------------------------------------------------
  -- Support types
  -- --------------------------------------------------------------------------
  Confidence = {VeryHigh, High, Medium, Low}

  Text = {
    Text_Desc_LinearAlgebra,
    Text_Desc_LieGroups,
    Text_Desc_ClassicalMechanics,
    Text_Desc_DifferentialGeometry,
    Text_Desc_DifferentialForms,
    Text_Desc_SymplecticGeometry,
    Text_Desc_HamiltonianMechanics,
    Text_Desc_SpecialRelativity,
    Text_Desc_GeneralRelativity,
    Text_Desc_CliffordAlgebras,
    Text_Desc_Spinors,
    Text_Desc_GaugeTheory,
    Text_Desc_QuantumFieldTheory,
    Text_Desc_Renormalization,
    Text_Desc_PathIntegral,
    Text_Desc_NoetherTheorem,
    Text_Ex_MaxwellAs2Form,
    Text_Ex_Schwarzschild,
    Text_Ex_SymplecticHO,
    Text_Guideline_Units,
    Text_Guideline_Signature,
    Text_Guideline_GaugeFixing,
    Text_Guideline_DistributionCare
  }

  -- --------------------------------------------------------------------------
  -- Learning / concept graph
  -- --------------------------------------------------------------------------
  Concept = {
    LinearAlgebra,
    LieGroups,
    ClassicalMechanics,
    DifferentialGeometry,
    DifferentialForms,
    SymplecticGeometry,
    HamiltonianMechanics,
    SpecialRelativity,
    GeneralRelativity,
    CliffordAlgebrasConcept,
    SpinorsConcept,
    GaugeTheoryConcept,
    QuantumFieldTheoryConcept,
    RenormalizationConcept,
    PathIntegralConcept,
    NoetherTheoremConcept
  }

  SafetyGuideline = {
    Guideline_CheckUnits,
    Guideline_TrackSignatureConvention,
    Guideline_GaugeFixingIsNotOptional,
    Guideline_DistributionsNeedRegularization
  }

  Example = {
    Example_MaxwellAs2Form,
    Example_SchwarzschildSpacetime,
    Example_SymplecticHarmonicOscillator
  }

  requires = {
    (concept=DifferentialForms, prereq=DifferentialGeometry),
    (concept=SymplecticGeometry, prereq=DifferentialForms),
    (concept=HamiltonianMechanics, prereq=SymplecticGeometry),
    (concept=SpecialRelativity, prereq=DifferentialGeometry),
    (concept=GeneralRelativity, prereq=DifferentialGeometry),
    (concept=CliffordAlgebrasConcept, prereq=LinearAlgebra),
    (concept=SpinorsConcept, prereq=CliffordAlgebrasConcept),
    (concept=GaugeTheoryConcept, prereq=LieGroups),
    (concept=QuantumFieldTheoryConcept, prereq=GaugeTheoryConcept),
    (concept=QuantumFieldTheoryConcept, prereq=SpinorsConcept),
    (concept=RenormalizationConcept, prereq=QuantumFieldTheoryConcept),
    (concept=PathIntegralConcept, prereq=QuantumFieldTheoryConcept),
    (concept=NoetherTheoremConcept, prereq=ClassicalMechanics)
  }

  explains = {
    (concept=DifferentialForms, guideline=Guideline_CheckUnits),
    (concept=SpecialRelativity, guideline=Guideline_TrackSignatureConvention),
    (concept=GaugeTheoryConcept, guideline=Guideline_GaugeFixingIsNotOptional),
    (concept=RenormalizationConcept, guideline=Guideline_DistributionsNeedRegularization)
  }

  conceptDescription = {
    (concept=LinearAlgebra, text=Text_Desc_LinearAlgebra),
    (concept=LieGroups, text=Text_Desc_LieGroups),
    (concept=ClassicalMechanics, text=Text_Desc_ClassicalMechanics),
    (concept=DifferentialGeometry, text=Text_Desc_DifferentialGeometry),
    (concept=DifferentialForms, text=Text_Desc_DifferentialForms),
    (concept=SymplecticGeometry, text=Text_Desc_SymplecticGeometry),
    (concept=HamiltonianMechanics, text=Text_Desc_HamiltonianMechanics),
    (concept=SpecialRelativity, text=Text_Desc_SpecialRelativity),
    (concept=GeneralRelativity, text=Text_Desc_GeneralRelativity),
    (concept=CliffordAlgebrasConcept, text=Text_Desc_CliffordAlgebras),
    (concept=SpinorsConcept, text=Text_Desc_Spinors),
    (concept=GaugeTheoryConcept, text=Text_Desc_GaugeTheory),
    (concept=QuantumFieldTheoryConcept, text=Text_Desc_QuantumFieldTheory),
    (concept=RenormalizationConcept, text=Text_Desc_Renormalization),
    (concept=PathIntegralConcept, text=Text_Desc_PathIntegral),
    (concept=NoetherTheoremConcept, text=Text_Desc_NoetherTheorem)
  }

  exampleDescription = {
    (example=Example_MaxwellAs2Form, text=Text_Ex_MaxwellAs2Form),
    (example=Example_SchwarzschildSpacetime, text=Text_Ex_Schwarzschild),
    (example=Example_SymplecticHarmonicOscillator, text=Text_Ex_SymplecticHO)
  }

  guidelineDescription = {
    (guideline=Guideline_CheckUnits, text=Text_Guideline_Units),
    (guideline=Guideline_TrackSignatureConvention, text=Text_Guideline_Signature),
    (guideline=Guideline_GaugeFixingIsNotOptional, text=Text_Guideline_GaugeFixing),
    (guideline=Guideline_DistributionsNeedRegularization, text=Text_Guideline_DistributionCare)
  }

  guidelineConfidence = {
    (guideline=Guideline_CheckUnits, conf=VeryHigh),
    (guideline=Guideline_TrackSignatureConvention, conf=High),
    (guideline=Guideline_GaugeFixingIsNotOptional, conf=High),
    (guideline=Guideline_DistributionsNeedRegularization, conf=Medium)
  }

  demonstrates = {
    (example=Example_MaxwellAs2Form, concept=DifferentialForms),
    (example=Example_MaxwellAs2Form, concept=GaugeTheoryConcept),
    (example=Example_SchwarzschildSpacetime, concept=GeneralRelativity),
    (example=Example_SymplecticHarmonicOscillator, concept=HamiltonianMechanics)
  }

  -- --------------------------------------------------------------------------
  -- Core math: small “anchors” so we can connect physics objects to structure.
  -- --------------------------------------------------------------------------
  Nat = {Nat0, Nat1, Nat2, Nat3, Nat4}
  ScalarFieldType = {RealField, ComplexField}

  LieGroup = {LorentzGroup_SO13, PoincareGroup}
  GaugeGroup = {GaugeGroup_U1, GaugeGroup_SU2, GaugeGroup_SU3}
  LieAlgebra = {LieAlg_so13, LieAlg_poincare, LieAlg_u1, LieAlg_su2, LieAlg_su3}

  LieGroupHasLieAlgebra = {
    (group=LorentzGroup_SO13, alg=LieAlg_so13),
    (group=PoincareGroup, alg=LieAlg_poincare),
    (group=GaugeGroup_U1, alg=LieAlg_u1),
    (group=GaugeGroup_SU2, alg=LieAlg_su2),
    (group=GaugeGroup_SU3, alg=LieAlg_su3)
  }

  -- --------------------------------------------------------------------------
  -- Geometry: manifolds + metrics + forms
  -- --------------------------------------------------------------------------
  SmoothManifold = {Manifold_S2, PhaseSpace_HO}
  LorentzianManifold = {MinkowskiSpacetime_M4, SchwarzschildSpacetime}
  Spacetime = {MinkowskiSpacetime_M4, SchwarzschildSpacetime}

  SymplecticManifold = {PhaseSpace_HO}
  PhaseSpace = {PhaseSpace_HO}

  ManifoldDimension = {
    (manifold=Manifold_S2, dim=Nat2),
    (manifold=PhaseSpace_HO, dim=Nat2),
    (manifold=MinkowskiSpacetime_M4, dim=Nat4),
    (manifold=SchwarzschildSpacetime, dim=Nat4)
  }

  Metric = {Metric_S2_Round, Metric_Minkowski, Metric_Schwarzschild}

  MetricOn = {
    (metric=Metric_S2_Round, manifold=Manifold_S2),
    (metric=Metric_Minkowski, manifold=MinkowskiSpacetime_M4),
    (metric=Metric_Schwarzschild, manifold=SchwarzschildSpacetime)
  }

  Connection = {Conn_LeviCivita_Minkowski, Conn_LeviCivita_Schwarzschild}
  LeviCivitaConnection = {
    (metric=Metric_Minkowski, connection=Conn_LeviCivita_Minkowski),
    (metric=Metric_Schwarzschild, connection=Conn_LeviCivita_Schwarzschild)
  }

  CurvatureTensor = {Curv_Minkowski, Curv_Schwarzschild}
  CurvatureOf = {
    (connection=Conn_LeviCivita_Minkowski, curvature=Curv_Minkowski),
    (connection=Conn_LeviCivita_Schwarzschild, curvature=Curv_Schwarzschild)
  }

  DifferentialForm = {Form_Maxwell_F, Form_Volume_M4}
  SymplecticForm = {Form_Symplectic_Omega_HO}

  FormOn = {
    (form=Form_Maxwell_F, manifold=MinkowskiSpacetime_M4),
    (form=Form_Volume_M4, manifold=MinkowskiSpacetime_M4),
    (form=Form_Symplectic_Omega_HO, manifold=PhaseSpace_HO)
  }

  FormDegree = {
    (form=Form_Maxwell_F, degree=Nat2),
    (form=Form_Volume_M4, degree=Nat4),
    (form=Form_Symplectic_Omega_HO, degree=Nat2)
  }

  SymplecticFormOn = {(form=Form_Symplectic_Omega_HO, manifold=PhaseSpace_HO)}
  SymplecticManifoldHasForm = {(manifold=PhaseSpace_HO, form=Form_Symplectic_Omega_HO)}

  -- Hodge star over Minkowski metric (toy; signature conventions matter).
  HodgeStar = {(metric=Metric_Minkowski, input=Form_Maxwell_F, output=Form_Maxwell_F_Dual)}

  -- --------------------------------------------------------------------------
  -- Mechanics: Hamiltonian on phase space
  -- --------------------------------------------------------------------------
  Hamiltonian = {Hamiltonian_HarmonicOscillator}
  VectorField = {VectorField_X_HO}

  TensorFieldOn = {(field=VectorField_X_HO, manifold=PhaseSpace_HO)}
  HamiltonianOn = {(H=Hamiltonian_HarmonicOscillator, phase=PhaseSpace_HO)}
  HamiltonianVectorFieldOf = {(H=Hamiltonian_HarmonicOscillator, X=VectorField_X_HO)}

  -- --------------------------------------------------------------------------
  -- Relativity: Einstein equation “hooks” + stress-energy
  -- --------------------------------------------------------------------------
  StressEnergyTensor = {StressEnergy_Vacuum}
  StressEnergyOn = {(T=StressEnergy_Vacuum, spacetime=SchwarzschildSpacetime)}

  EinsteinEquation = {EinsteinEq_Vacuum_Schwarzschild}
  EinsteinEquationOn = {
    (eq=EinsteinEq_Vacuum_Schwarzschild, spacetime=SchwarzschildSpacetime, stress=StressEnergy_Vacuum)
  }

  -- --------------------------------------------------------------------------
  -- Clifford algebras: gamma matrices attached to Minkowski metric
  -- --------------------------------------------------------------------------
  CliffordAlgebra = {Clifford_Minkowski}
  AlgebraOver = {(algebra=Clifford_Minkowski, field=ComplexField)}
  CliffordAlgebraFromMetric = {(metric=Metric_Minkowski, alg=Clifford_Minkowski)}

  GammaMatrix = {Gamma_0, Gamma_1, Gamma_2, Gamma_3}
  GammaMatrixInAlgebra = {
    (gamma=Gamma_0, alg=Clifford_Minkowski),
    (gamma=Gamma_1, alg=Clifford_Minkowski),
    (gamma=Gamma_2, alg=Clifford_Minkowski),
    (gamma=Gamma_3, alg=Clifford_Minkowski)
  }

  -- --------------------------------------------------------------------------
  -- QFT: QED as a gauge theory on Minkowski spacetime (high-level)
  -- --------------------------------------------------------------------------
  QuantumFieldTheory = {QFT_QED}
  QuantumField = {Field_A_mu, Field_psi}
  LagrangianDensity = {Lag_QED}
  ActionFunctional = {Action_QED}

  QFTOnSpacetime = {(qft=QFT_QED, spacetime=MinkowskiSpacetime_M4)}
  QFTHasField = {(qft=QFT_QED, field=Field_A_mu), (qft=QFT_QED, field=Field_psi)}
  QFTHasGaugeGroup = {(qft=QFT_QED, group=GaugeGroup_U1)}
  QFTHasLagrangian = {(qft=QFT_QED, lag=Lag_QED)}
  LagrangianDefinesAction = {(lag=Lag_QED, action=Action_QED)}
